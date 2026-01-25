# Web Dashboard

Thunderbot includes a web-based control deck for monitoring and interacting with the bot outside of the Bluesky app.

## Vision

The Control Deck is a dedicated Bluesky client for chatting with Thunderbot. When you send a message through the web interface, it posts to Bluesky mentioning `@thunderbot.bsky.social`. The bot responds normally via Jetstream, and the conversation exists on both platforms.

This approach:

- Uses the existing thread tracking infrastructure
- Conversations visible in Bluesky app and web UI
- No separate chat database needed
- Can continue conversations from either platform

The interface uses Pico CSS with a developer-focused aesthetic: JetBrains Mono for body text, Lora for headings.

## Starting the Dashboard

```bash
export DASHBOARD_TOKEN=your-secure-token
export ALLOWED_HANDLES=desertthunder.dev,stormlightlabs.org
thunderbot serve
```

The dashboard is available at `http://127.0.0.1:3000`

## Authentication

Users log in with their BlueSky handle and app password. The web server validates credentials against the PDS and stores the session to post on the user's behalf.

Access is restricted to handles listed in `ALLOWED_HANDLES`. This keeps the deployment simple for personal or small-team use.

Session data:

- Stored encrypted in cookie or database
- Includes `access_jwt` for posting as the user
- Refreshed automatically when expired
- 8-hour cookie expiration

## Sections

### Status

Bot health at a glance:

- Connection status (Jetstream, Bluesky session)
- Message counts (conversations, threads, identities)
- Last activity timestamp
- Pause/resume controls

Status widgets refresh automatically every 5 seconds via HTMX.

### Chat

A dedicated interface for chatting with ThunderBot. Messages you send are posted to Bluesky mentioning `@thunderbot.bsky.social`.

How it works:

1. You type a message in the chat UI
2. Web server posts it as you, mentioning the bot
3. Jetstream picks up the mention
4. Bot generates and posts a response
5. Response appears in your thread

Features:

- Shows your conversation threads with the bot
- Real-time updates via HTMX polling
- Character counter (300 limit)
- Continue conversations started in Bluesky app

The chat view filters threads to only show conversations between you and the bot.

### Threads

Browse all Bluesky conversations the bot has participated in:

- List recent threads grouped by root URI
- Click to expand full message history
- User messages vs. bot responses styled as chat bubbles
- Timestamps and latency metrics

This is the admin view showing all threads, not just yours.

### Broadcast

Post to Bluesky as the bot account (admin only):

- Compose and send posts
- Preview before posting
- View recent posts

Useful for announcements or posts that shouldn't be replies.

### Config (Planned)

Bot configuration and controls:

- Pause/resume event processing
- Clear context for specific threads
- View/update system prompt
- Connection diagnostics

## Typography

```css
/* Headings */
font-family: "Lora", serif;

/* Body and data */
font-family: "JetBrains Mono", monospace;
```

Fonts loaded from Google Fonts CDN. The Pico CSS jade theme provides the color palette and component styling.

## Current Implementation

- Landing page with dashboard link
- Status page with statistics
- Threads list and detail view
- Identities table
- Admin page with post form and pause/resume buttons
- Bearer token authentication via `DASHBOARD_TOKEN`
- HTMX integration for dynamic updates
- Pico CSS jade theme

## Planned Features

1. BlueSky authentication (post as logged-in user)
2. Chat interface that posts mentions to the bot
3. User-filtered thread view
4. Typography update (JetBrains Mono + Lora)
5. Unified navigation between sections
6. Configuration panel

## Environment Variables

| Variable          | Required | Description                                      |
| ----------------- | -------- | ------------------------------------------------ |
| `DASHBOARD_TOKEN` | Yes      | Secret for cookie signing/encryption             |
| `ALLOWED_HANDLES` | Yes      | Comma-separated handles allowed access           |
| `PDS_HOST`        | No       | BlueSky PDS URL (default: `https://bsky.social`) |

## Routes

Current:

- `GET /` - Landing page
- `GET /dashboard` - Status and statistics
- `GET /threads` - Thread list (all)
- `GET /thread/:id` - Thread detail
- `GET /identities` - Identity cache
- `GET /admin` - Admin controls
- `POST /api/post` - Create Bluesky post (as bot)
- `POST /api/pause` - Pause bot
- `POST /api/resume` - Resume bot

Planned:

- `GET /login` - Login form
- `POST /login` - Authenticate with BlueSky
- `GET /chat` - Chat interface (your threads with bot)
- `POST /chat/send` - Post message mentioning bot
- `POST /logout` - End session
