# Gemini Integration

Technical details for Google Gemini API integration.

## API Configuration

### Endpoint

```text
https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent
```

### Environment Variables

| Variable         | Description      | Default                |
| ---------------- | ---------------- | ---------------------- |
| `GEMINI_API_KEY` | Google API key   | Required               |
| `GEMINI_MODEL`   | Model identifier | `gemini-3-pro-preview` |

### Available Models

| Model                    | Free Tier | RPM | RPD  | Context |
| ------------------------ | --------- | --- | ---- | ------- |
| `gemini-3-pro-preview`   | Yes       | ~5  | ~100 | 1M+     |
| `gemini-3-flash-preview` | Yes       | ~15 | ~500 | 1M+     |
| `gemini-2.5-pro`         | Yes       | 5   | 100  | 1M      |
| `gemini-2.5-flash`       | Yes       | 15  | 500  | 1M      |

RPM = Requests per minute, RPD = Requests per day

Free tier is unavailable in EU, UK, and Switzerland.

## Request Format

### Content Structure

```json
{
    "contents": [
        {
            "role": "user",
            "parts": [{ "text": "Hello" }]
        },
        {
            "role": "model",
            "parts": [{ "text": "Hi there!" }]
        }
    ],
    "systemInstruction": {
        "parts": [{ "text": "You are a helpful assistant." }]
    },
    "generationConfig": {
        "temperature": 0.7,
        "topP": 0.9,
        "topK": 40,
        "maxOutputTokens": 1024
    }
}
```

### Role Mapping

Gemini requires strict `user`/`model` roles:

- Incoming Bluesky posts from others -> `user`
- Bot's previous replies -> `model`

For multi-user threads, prepend handle to distinguish speakers:

```text
[@alice.bsky.social]: What do you think?
```

## Response Handling

### Response Structure

```json
{
    "candidates": [
        {
            "content": {
                "role": "model",
                "parts": [{ "text": "Response text here" }]
            }
        }
    ],
    "usageMetadata": {
        "promptTokenCount": 100,
        "candidatesTokenCount": 50,
        "totalTokenCount": 150
    }
}
```

Extract text from the first candidate's first part.

### Token Usage Logging

The client logs token counts for monitoring:

```text
Tokens used: prompt=100, candidates=50, total=150
```

## Retry Strategy

### Server Errors (5xx)

- Max retries: 3
- Backoff: 1s, 2s, 4s (exponential)
- Retries on: 500, 502, 503, 504

### Client Errors (4xx)

No automatic retry. Log error and propagate:

- **400**: Invalid request (check payload)
- **401**: Invalid API key
- **429**: Rate limited (quota exceeded)

## Prompt Builder

### Building Context

The `PromptBuilder` formats conversation history for Gemini:

1. Fetch thread history from database by `thread_root_uri`
2. Map each message to `Content` with appropriate role
3. Prepend handles for non-bot users
4. Include system instruction (constitution)

### System Instruction

The constitution defines the agent's persona and is included as `systemInstruction` in every request.

### Handling Empty Threads

When no thread history exists, degrade to one-shot mode. The current message becomes the only content in the request.

## Generation Config

Default parameters:

| Parameter         | Value | Purpose                    |
| ----------------- | ----- | -------------------------- |
| `temperature`     | 0.7   | Response randomness        |
| `topP`            | 0.9   | Nucleus sampling threshold |
| `topK`            | 40    | Token selection pool       |
| `maxOutputTokens` | 1024  | Response length limit      |

### Thinking Mode

Gemini 3 supports a "thinking" mode where internal reasoning is exposed. Currently disabled for production:

```json
"thinkingConfig": {
  "includeThoughts": false
}
```

Enable for debugging to see the model's reasoning process.

## Model Recommendations

### Production (Default)

`gemini-3-pro-preview`: Best reasoning quality for complex conversation threads.

### High Throughput

`gemini-3-flash-preview`: Faster responses, higher rate limits for simple queries.

### Stability

`gemini-2.5-pro`: Stable release without preview limitations.

### Cost/Speed

`gemini-2.5-flash`: Lowest latency, suitable for testing and development.

## Error Messages

### Common Errors

| Code | Error             | Resolution                      |
| ---- | ----------------- | ------------------------------- |
| 400  | InvalidRequest    | Check request payload format    |
| 401  | Unauthenticated   | Verify API key                  |
| 403  | PermissionDenied  | Check model access/region       |
| 404  | ModelNotFound     | Verify model ID spelling        |
| 429  | ResourceExhausted | Wait and retry, or reduce usage |
| 500  | InternalError     | Retry with backoff              |

### Regional Restrictions

Free tier is unavailable in:

- European Union
- United Kingdom
- Switzerland

Users in these regions require a paid Google Cloud billing account.
