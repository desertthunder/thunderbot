# Jetstream Integration

Jetstream is a streaming service by Bluesky that consumes the AT Protocol `com.atproto.sync.subscribeRepos` firehose and re-emits it as lightweight JSON over a WebSocket.
It strips CBOR-encoded MST blocks, yielding ~99% less data than the raw firehose.
No authentication is required to consume it.

## Public Instances

Four official instances, interchangeable via time-based cursors:

| Host                              | Region  |
| --------------------------------- | ------- |
| `jetstream1.us-east.bsky.network` | US East |
| `jetstream2.us-east.bsky.network` | US East |
| `jetstream1.us-west.bsky.network` | US West |
| `jetstream2.us-west.bsky.network` | US West |

Connect via: `wss://<host>/subscribe`

## Query Parameters

| Param                 | Type       | Default   | Description                                                                            |
| --------------------- | ---------- | --------- | -------------------------------------------------------------------------------------- |
| `wantedCollections`   | `string[]` | all       | Collection NSIDs to filter (max 100). Supports NSID path prefixes (`app.bsky.feed.*`). |
| `wantedDids`          | `string[]` | all       | Repo DIDs to filter (max 10,000).                                                      |
| `cursor`              | `i64`      | live-tail | Unix microseconds timestamp to replay from. Absent or future = live-tail.              |
| `compress`            | `bool`     | `false`   | Enable zstd compression (custom dictionary required).                                  |
| `maxMessageSizeBytes` | `i64`      | 0 (none)  | Max payload size the client will accept.                                               |
| `requireHello`        | `bool`     | `false`   | Pause replay until client sends an `options_update` message.                           |

> [!IMPORTANT]
> `identity` and `account` events are **always** delivered regardless of
> `wantedCollections`.

## Event Schema

Every event is a JSON object with these common fields:

```json
{
  "did": "did:plc:...",
  "time_us": 1725911162329308,
  "kind": "commit" | "identity" | "account",
  ...
}
```

### `commit`

Repository record change. Operations: `create`, `update`, `delete`.

```json
{
  "did": "did:plc:eygmaihciaxprqvxpfvl6flk",
  "time_us": 1725911162329308,
  "kind": "commit",
  "commit": {
    "rev": "3l3qo2vutsw2b",
    "operation": "create",
    "collection": "app.bsky.feed.like",
    "rkey": "3l3qo2vuowo2b",
    "record": { "$type": "app.bsky.feed.like", "...": "..." },
    "cid": "bafyrei..."
  }
}
```

On `delete`, only `rev`, `operation`, `collection`, and `rkey` are present (no
`record` or `cid`).

### `identity`

DID document / handle change. Signals that cached identity data should be
re-resolved.

```json
{
  "did": "did:plc:...",
  "time_us": 1725516665234703,
  "kind": "identity",
  "identity": {
    "did": "did:plc:...",
    "handle": "user.bsky.social",
    "seq": 1409752997,
    "time": "2024-09-05T06:11:04.870Z"
  }
}
```

### `account`

Account status change (active, deactivated, takendown).

```json
{
  "did": "did:plc:...",
  "time_us": 1725516665333808,
  "kind": "account",
  "account": {
    "active": true,
    "did": "did:plc:...",
    "seq": 1409753013,
    "time": "2024-09-05T06:11:04.870Z"
  }
}
```

## Compression

Jetstream supports **zstd** compression with a **custom dictionary** (found in
the Jetstream repo at `pkg/models/zstd_dictionary`). Compressed messages are
~56% smaller than raw JSON.

Enable via:

- Query param: `compress=true`
- Header: `Socket-Encoding: zstd`

### Rust Implementation

Use `zstd` crate with the dictionary bytes embedded at compile time:

```rust
static ZSTD_DICT: &[u8] = include_bytes!("zstd_dictionary");

fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = zstd::bulk::Decompressor::with_dictionary(ZSTD_DICT)?;
    Ok(decoder.decompress(data, 10 * 1024 * 1024)?) // 10 MiB limit
}
```

> [!NOTE]
> The dictionary must be fetched from the Jetstream repo and vendored into the
> project. It is required for decoding; standard zstd without the dictionary
> will fail.

## Subscriber-Sourced Messages

Clients can send JSON text frames to update filters after connecting:

```json
{
  "type": "options_update",
  "payload": {
    "wantedCollections": ["app.bsky.feed.post"],
    "wantedDids": ["did:plc:..."],
    "maxMessageSizeBytes": 1000000
  }
}
```

Empty arrays **disable** the corresponding filter (i.e., receive everything).
Max message size: 10 MiB. Invalid payloads disconnect the client.

## Cursor Semantics

- Cursors are **Unix microseconds** (`time_us` field on every event).
- Absent or future cursor → live-tail.
- On reconnect, rewind cursor by a few seconds for gapless playback (assuming
  idempotent processing).
- Same cursor works across all public instances (time-based, not
  sequence-based).

## Thunderbot Integration Notes

### Connection Setup

```text
wss://jetstream2.us-east.bsky.network/subscribe
  ?wantedCollections=app.bsky.feed.post
  &compress=true
```

### Mention Detection

Filter incoming `commit` events where `collection == "app.bsky.feed.post"` and
`operation == "create"`. Then inspect `record.facets[]` for facets of type
`app.bsky.richtext.facet#mention` whose `did` matches the bot's DID.

### Reconnection Strategy

1. Persist `time_us` of last successfully processed event.
2. On disconnect, reconnect with `cursor = last_time_us - 5_000_000` (5s
   buffer).
3. Exponential backoff: 1s initial, 2x factor, 60s max, with jitter.
4. Deduplicate replayed events via `(did, rkey)` or `post_uri`.
