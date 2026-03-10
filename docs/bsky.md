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

### OAuth 2.0 (AT Protocol Profile)

AT Protocol defines an OAuth 2.0 profile intended to replace `createSession` /
app passwords. It enables third-party authorization without credential sharing.

#### Mandatory Mechanisms

- **DPoP** (RFC 9449) — tokens are bound to a per-session ES256 keypair. A
  unique DPoP JWT (`jti` = random) must be signed for every request. Servers
  provide `DPoP-Nonce` headers that clients must track per server per session.
- **PKCE** (RFC 7636) — S256 only, `plain` is disallowed.
- **PAR** (RFC 9126) — all authorization requests must use Pushed Authorization
  Requests.

#### Client Types

| Type             | Key                                  | Refresh Token Lifetime |
| ---------------- | ------------------------------------ | ---------------------- |
| **Confidential** | Server-side signing key (JWKS)       | ≤ 180 days             |
| **Public**       | No signing key (SPA, mobile, CLI)    | ≤ 2 weeks              |

Access tokens are < 15 min (< 5 min recommended if non-revocable). Refresh
tokens are **single-use** — each refresh returns a new one.

#### Scopes

The `atproto` scope is required for all sessions. Transitional scopes:

- `transition:generic` — broad PDS access (will be replaced by granular perms)
- `transition:chat.bsky` — DM access

The Authorization Server returns the account DID in the `sub` field.

#### Authorization Flow (Summary)

```text
1. Resolve user handle/DID → PDS → Authorization Server metadata
2. PAR (POST) with: client_id, redirect_uri, scope, state, code_challenge, DPoP
3. Server returns request_uri
4. Redirect user to authorization_endpoint?request_uri=...&client_id=...
5. User authenticates and approves
6. Callback: redirect_uri?code=...&state=...&iss=...
7. Exchange code for tokens (POST token_endpoint) with code_verifier + DPoP
8. Verify sub (DID) by resolving DID doc → PDS → issuer match
```

#### Client Metadata Document

Hosted at the `client_id` URL:

```json
{
  "client_id": "https://example.com/client-metadata.json",
  "client_name": "My App",
  "redirect_uris": ["https://example.com/callback"],
  "grant_types": ["authorization_code", "refresh_token"],
  "response_types": ["code"],
  "scope": "atproto transition:generic",
  "token_endpoint_auth_method": "private_key_jwt",
  "dpop_bound_access_tokens": true,
  "jwks_uri": "https://example.com/jwks.json"
}
```

For localhost development, `client_id` can use `http://localhost`.

> [!NOTE]
> `createSession` with app passwords is simpler and sufficient for first-party bots.
> OAuth is needed when acting on behalf of other users or when longer session lifetimes are required.

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
