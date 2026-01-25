# Protocol Reference

Technical details for Jetstream and XRPC integration.

## Jetstream Protocol

Jetstream is a simplified JSON event stream for AT Protocol. It converts CBOR-encoded firehose data into lightweight JSON over WebSocket.

### Connection Endpoints

| Region  | Endpoint                                          |
| ------- | ------------------------------------------------- |
| US East | `wss://jetstream2.us-east.bsky.network/subscribe` |
| US West | `wss://jetstream2.us-west.bsky.network/subscribe` |

### Query Parameters

| Parameter           | Description                                | Limit      |
| ------------------- | ------------------------------------------ | ---------- |
| `wantedCollections` | NSID array to filter records               | 100 max    |
| `wantedDids`        | DID array to filter by repo                | 10,000 max |
| `cursor`            | Unix microseconds timestamp to replay from | -          |
| `compress`          | Set to `true` for zstd compression         | -          |

Example URL:

```text
wss://jetstream2.us-east.bsky.network/subscribe?wantedCollections=app.bsky.feed.post&compress=true
```

### Event Types

All events share a common envelope with `kind` discriminator:

- **commit**: Repository operations (create, update, delete)
- **identity**: Handle changes
- **account**: Account status changes

### Commit Event Structure

```json
{
  "kind": "commit",
  "did": "did:plc:...",
  "time_us": 1706140800000000,
  "commit": {
    "rev": "...",
    "operation": "create",
    "collection": "app.bsky.feed.post",
    "rkey": "3kf...",
    "record": { ... },
    "cid": "bafyrei..."
  }
}
```

### Post Record Structure

```json
{
  "text": "Hello @bot.bsky.social",
  "createdAt": "2024-01-25T00:00:00.000Z",
  "facets": [
    {
      "index": { "byteStart": 6, "byteEnd": 22 },
      "features": [
        {
          "$type": "app.bsky.richtext.facet#mention",
          "did": "did:plc:..."
        }
      ]
    }
  ],
  "reply": {
    "root": { "uri": "at://...", "cid": "bafyrei..." },
    "parent": { "uri": "at://...", "cid": "bafyrei..." }
  }
}
```

### Mention Detection

Mentions are encoded in the `facets` array with feature type
`app.bsky.richtext.facet#mention`. The `did` field identifies the mentioned user.

### Reconnection Strategy

Exponential backoff with jitter:

- Initial delay: 1 second
- Maximum delay: 60 seconds
- Factor: 2x
- Jitter: 0-25% of current delay

The `time_us` field serves as a cursor for replay after reconnection.

## XRPC Protocol

XRPC (Cross-organizational Remote Procedure Calls) is the HTTP API layer for AT Protocol.

### Endpoint Structure

- **Base URL**: User's PDS (e.g., `https://bsky.social`)
- **Path**: `/xrpc/{nsid}`
- **Method**: GET for queries, POST for procedures

### Authentication

Create a session with handle and app password:

```json
POST /xrpc/com.atproto.server.createSession
{
  "identifier": "handle.bsky.social",
  "password": "app-password"
}
```

Response contains `accessJwt` and `refreshJwt`. Include access token in subsequent requests:

```text
Authorization: Bearer {accessJwt}
```

Refresh tokens when access expires:

```text
POST /xrpc/com.atproto.server.refreshSession
Authorization: Bearer {refreshJwt}
```

### Creating Posts

```text
POST /xrpc/com.atproto.repo.createRecord
Authorization: Bearer {accessJwt}
```

```json
{
  "repo": "did:plc:...",
  "collection": "app.bsky.feed.post",
  "record": {
    "$type": "app.bsky.feed.post",
    "text": "Hello world",
    "createdAt": "2024-01-25T00:00:00.000Z",
    "reply": {
      "root": { "uri": "at://...", "cid": "bafyrei..." },
      "parent": { "uri": "at://...", "cid": "bafyrei..." }
    }
  }
}
```

Response:

```json
{
  "uri": "at://did:plc:.../app.bsky.feed.post/3kf...",
  "cid": "bafyrei..."
}
```

### Fetching Posts

```text
GET /xrpc/com.atproto.repo.getRecord?repo={did}&collection=app.bsky.feed.post&rkey={rkey}
Authorization: Bearer {accessJwt}
```

### Resolving Handles

No authentication required:

```text
GET /xrpc/com.atproto.identity.resolveHandle?handle=user.bsky.social
```

Response:

```json
{ "did": "did:plc:..." }
```

### Error Handling

XRPC errors return JSON with `error` and `message` fields:

```json
{
  "error": "InvalidRequest",
  "message": "..."
}
```

Common errors:

- **401**: Authentication required or token expired
- **429**: Rate limited (implement backoff)
- **400**: Invalid request parameters

### Rate Limiting

Bluesky enforces rate limits. On 429 responses:

- Initial retry: 2 seconds
- Maximum retries: 3
- Backoff: 2s, 4s, 8s

### AT URI Format

Posts are identified by AT URIs:

```text
at://{did}/{collection}/{rkey}
at://did:plc:xyz/app.bsky.feed.post/3kfabc
```

Parse by splitting on `/` after `at://`.
