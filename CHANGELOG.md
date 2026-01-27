# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.8.0] - 2026-01-26

### Added

- Observability system with `/health` endpoint, real-time dashboard status, and CLI diagnostics.
- Dev tools with `--dry-run` modes, structured log streaming, and database backup/restore commands.

## [0.7.0] - 2026-01-25

### Added

- BlueSky authentication with encrypted session cookies and auto-refresh
- Chat interface for posting messages mentioning @thunderbot.bsky.social
- User-specific thread filtering and reply support
- JetBrains Mono and Lora fonts for developer-focused typography
- Unified navigation sidebar with Status, Chat, Threads, Broadcast, and Config sections
- Configuration panel with pause/resume, thread context clearing, and connection diagnostics
- Mention facet support and character counter for chat messages

### Changed

- Moved pause/resume controls from Admin page to dedicated Config section
- Updated post creation to use PostRecordWrite builder pattern

## [0.6.0] - 2026-01-25

### Changed

- Unified vector and metadata storage into SQLite using sqlite-vec extension
- Replaced LanceDB with FTS5 + vector hybrid search

## [0.5.0] - 2026-01-25

### Added

- Vector-based RAG system for semantic memory retrieval
- Embedding generation via Gemini API
- CLI commands for vector operations

## [0.4.0] - 2026-01-24

### Added

- Gemini API integration with prompt builder and conversation context
- Agent orchestration for processing mentions and posting replies
- Loop prevention and silent mode support

## [0.3.0] - 2026-01-23

### Added

- Bluesky XRPC client for authentication, posting, and identity resolution
- Session persistence in database with file fallback

## [0.2.0] - 2026-01-22

### Added

- libSQL database for conversation storage and thread reconstruction
- Identity resolution cache with TTL

## [0.1.0] - 2026-01-21

### Added

- Cargo workspace with CLI and core library crates
- Jetstream WebSocket client with zstd compression
- Mention filtering and event processing pipeline
