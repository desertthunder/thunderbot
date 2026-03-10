# Bluesky XRPC Integration

The AT Protocol uses XRPC (Lexicon RPC) — HTTP endpoints at `/xrpc/<method>` on the user's PDS.
All authenticated requests use `Authorization: Bearer <accessJwt>`.
All request/response bodies are JSON.

## Authentication

### `com.atproto.server.createSession`

**POST** `/xrpc/com.atproto.server.createSession`

Request:

```json
{
  "identifier": "<handle-or-did>",
  "password": "<app-password>"
}
```

Response (200):

```json
{
  "did": "did:plc:...",
  "handle": "user.bsky.social",
  "email": "user@example.com",
  "accessJwt": "<short-lived>",
  "refreshJwt": "<long-lived>",
  "didDoc": { "...": "..." }
}
```

Errors: `401 AuthenticationRequired`, `400 InvalidRequest`

### `com.atproto.server.refreshSession`

**POST** `/xrpc/com.atproto.server.refreshSession`

- Auth header uses the **refreshJwt** (not accessJwt).
- Returns a fresh `accessJwt` and `refreshJwt` pair.
- Proactively refresh before expiry; decode the JWT `exp` claim to track.

### Session Lifecycle

```text
createSession → accessJwt (short-lived, ~2h)
                refreshJwt (long-lived, ~90d)

On accessJwt expiry → call refreshSession with refreshJwt
On refreshJwt expiry → call createSession again
```

## Record Operations

### `com.atproto.repo.createRecord`

**POST** `/xrpc/com.atproto.repo.createRecord` (auth required)

Request:

```json
{
  "repo": "<did>",
  "collection": "app.bsky.feed.post",
  "record": {
    "$type": "app.bsky.feed.post",
    "text": "Hello World!",
    "createdAt": "2024-01-01T00:00:00.000Z",
    "langs": ["en"],
    "facets": [],
    "reply": null
  }
}
```

Response (200):

```json
{
  "uri": "at://did:plc:.../app.bsky.feed.post/3k4duaz5vfs2b",
  "cid": "bafyrei..."
}
```

> [!IMPORTANT]
> The response `uri` and `cid` form the **strong reference** needed by future
> replies. Store both immediately after posting.

### `com.atproto.repo.getRecord`

**GET** `/xrpc/com.atproto.repo.getRecord` (public, no auth needed)

Query params: `repo`, `collection`, `rkey` (optionally `cid` for version pinning).

Response (200):

```json
{
  "uri": "at://did:plc:.../app.bsky.feed.post/<rkey>",
  "cid": "bafyrei...",
  "value": { "$type": "app.bsky.feed.post", "text": "...", "...": "..." }
}
```

## Post Record Schema

`app.bsky.feed.post` record fields:

| Field       | Type       | Required | Description                                   |
| ----------- | ---------- | -------- | --------------------------------------------- |
| `$type`     | `string`   | ✓        | Always `"app.bsky.feed.post"`                 |
| `text`      | `string`   | ✓        | Post content (max 300 graphemes)              |
| `createdAt` | `string`   | ✓        | ISO 8601 timestamp, prefer trailing `Z`       |
| `langs`     | `string[]` | ✗        | BCP-47 language tags                          |
| `facets`    | `Facet[]`  | ✗        | Rich text annotations (mentions, links, tags) |
| `reply`     | `ReplyRef` | ✗        | Thread reference (root + parent)              |
| `embed`     | `union`    | ✗        | Images, external links, quote posts           |

## Reply Threading

A reply requires a `ReplyRef` with **strong references** (URI + CID) to both
the thread root and the immediate parent:

```json
{
  "reply": {
    "root": {
      "uri": "at://did:plc:.../app.bsky.feed.post/<rkey>",
      "cid": "bafyrei..."
    },
    "parent": {
      "uri": "at://did:plc:.../app.bsky.feed.post/<rkey>",
      "cid": "bafyrei..."
    }
  }
}
```

**Resolution logic:**

1. Fetch the parent post via `getRecord`.
2. If parent has a `reply.root`, use that as root.
3. If parent has no `reply` field, it is the root — use the parent as both root
   and parent.

## Facets (Rich Text)

Facets annotate byte ranges in the post text. Indices are **UTF-8 byte
offsets** (start inclusive, end exclusive).

### Mention

```json
{
  "index": { "byteStart": 0, "byteEnd": 12 },
  "features": [
    {
      "$type": "app.bsky.richtext.facet#mention",
      "did": "did:plc:..."
    }
  ]
}
```

> [!WARNING]
> Mentions reference a **DID**, not a handle. The handle must be resolved to a
> DID via `resolveHandle` before constructing the facet.

### Link

```json
{
  "index": { "byteStart": 20, "byteEnd": 50 },
  "features": [
    {
      "$type": "app.bsky.richtext.facet#link",
      "uri": "https://example.com"
    }
  ]
}
```

### Tag (Hashtag)

```json
{
  "index": { "byteStart": 0, "byteEnd": 5 },
  "features": [
    {
      "$type": "app.bsky.richtext.facet#tag",
      "tag": "rust"
    }
  ]
}
```

## Identity Resolution

### `com.atproto.identity.resolveHandle`

**GET** `/xrpc/com.atproto.identity.resolveHandle?handle=<handle>`

Response: `{ "did": "did:plc:..." }`

No auth required. Use for converting `@handle` mentions to DIDs.

### `app.bsky.actor.getProfile`

**GET** `/xrpc/app.bsky.actor.getProfile?actor=<did-or-handle>`

Returns display name, avatar, description, follower/following counts.

## AT URI Format

```text
at://<did>/<collection>/<rkey>
```

Example: `at://did:plc:abc123/app.bsky.feed.post/3k43tv4rft22g`

Parse with: split on `/`, segments 2=repo, 3=collection, 4=rkey.

## Rate Limits

| Category                | Limit                                    | Scope   |
| ----------------------- | ---------------------------------------- | ------- |
| Writes (createRecord)   | 3 pts each, 5,000 pts/hr, 35,000 pts/day | Per DID |
| Reads (getRecord, etc.) | 3,000 req / 5 min                        | Per IP  |
| Auth (createSession)    | 30 / 5 min                               | Per IP  |

HTTP `429 Too Many Requests` is returned when limits are exceeded. Implement
exponential backoff with jitter.

## Error Handling

All XRPC errors return:

```json
{
  "error": "ErrorName",
  "message": "Human-readable description"
}
```

| Status | Error                    | Action                       |
| ------ | ------------------------ | ---------------------------- |
| `400`  | `InvalidRequest`         | Fix request payload          |
| `401`  | `AuthenticationRequired` | Refresh or recreate session  |
| `403`  | `Forbidden`              | Account suspended/taken down |
| `429`  | `RateLimitExceeded`      | Backoff and retry            |
| `500+` | Server error             | Retry with backoff           |
