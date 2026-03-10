# Cognition (GLM-5 via Z.ai)

## API Endpoints

Z.ai exposes two base URLs. Both are OpenAI-API-compatible:

| Endpoint                              | Purpose                               | Billing                     |
| ------------------------------------- | ------------------------------------- | --------------------------- |
| `https://api.z.ai/api/coding/paas/v4` | Coding Plan — coding tools only       | Deducted from plan quota    |
| `https://api.z.ai/api/paas/v4`        | General API — standalone/direct calls | Billed per-token separately |

Both expose `/chat/completions`. Auth: `Authorization: Bearer <API_KEY>`.

The **coding plan** endpoint is for use within supported tools (Claude Code, Kilo Code, Cline, OpenCode).
Calls from arbitrary binaries should use the **general API** endpoint.
API calls are billed separately and do not consume coding plan quota.

### Coding Plan Quota

GLM-5 consumes more plan quota than GLM-4.7:

- **Peak hours** (14:00–18:00 UTC+8): 3× standard rate
- **Off-peak**: 2× standard rate
- Available on **Pro** and **Max** plans; rolling out to Lite

## Model Specifications

| Property       | Value                        |
| -------------- | ---------------------------- |
| Model name     | `glm-5`                      |
| Architecture   | MoE — 744B total, 40B active |
| Training       | 28.5T tokens                 |
| Context window | 200K tokens                  |
| Max output     | 128K tokens                  |
| SDK compat     | OpenAI Python, Node.js, Java |

## Chat Completions

### Request

```json
{
  "model": "glm-5",
  "messages": [
    { "role": "system", "content": "<constitution>" },
    { "role": "user", "content": "[@alice.bsky.social]: Hello bot!" },
    { "role": "assistant", "content": "Greetings, Alice." },
    { "role": "user", "content": "[@bob.bsky.social]: What did Alice say?" }
  ],
  "temperature": 0.7,
  "max_tokens": 300,
  "stream": false
}
```

### Response

```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion",
  "model": "glm-5",
  "choices": [
    {
      "index": 0,
      "message": { "role": "assistant", "content": "..." },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 1200,
    "completion_tokens": 85,
    "total_tokens": 1285
  }
}
```

### Supported Parameters

| Parameter         | Type             | Default  | Notes                                                    |
| ----------------- | ---------------- | -------- | -------------------------------------------------------- |
| `model`           | `string`         | required | `"glm-5"`                                                |
| `messages`        | `Message[]`      | required | System, user, assistant, tool roles                      |
| `temperature`     | `float`          | `0.7`    | 0.0–1.0                                                  |
| `max_tokens`      | `int`            | model    | Max output tokens                                        |
| `stream`          | `bool`           | `false`  | SSE streaming                                            |
| `tools`           | `Tool[]`         | `null`   | Function definitions for tool use                        |
| `tool_choice`     | `string\|object` | `"auto"` | Only `"auto"` supported currently                        |
| `tool_stream`     | `bool`           | `false`  | Stream tool call argument construction (requires stream) |
| `response_format` | `object`         | `null`   | `{"type": "json_object"}` for JSON mode                  |
| `thinking`        | `object`         | `null`   | `{"type": "enabled"}` for deep reasoning                 |

## Function Calling

GLM-5 supports OpenAI-style function calling. The model receives tool
definitions and autonomously decides when to invoke them.

### Tool Definition

```json
{
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "get_thread_context",
        "description": "Fetch historical context for a conversation thread",
        "parameters": {
          "type": "object",
          "properties": {
            "root_uri": {
              "type": "string",
              "description": "AT URI of thread root"
            }
          },
          "required": ["root_uri"]
        }
      }
    }
  ],
  "tool_choice": "auto"
}
```

### Tool Call Response

When the model decides to use a tool, `finish_reason` is `"tool_calls"`:

```json
{
  "choices": [
    {
      "message": {
        "role": "assistant",
        "content": null,
        "tool_calls": [
          {
            "id": "call_abc123",
            "type": "function",
            "function": {
              "name": "get_thread_context",
              "arguments": "{\"root_uri\": \"at://did:plc:.../app.bsky.feed.post/xxx\"}"
            }
          }
        ]
      },
      "finish_reason": "tool_calls"
    }
  ]
}
```

### Returning Tool Results

Execute the function, then send the result back as a `tool` message:

```json
{ "role": "tool", "tool_call_id": "call_abc123", "content": "<JSON result>" }
```

The full message history (including the assistant's tool call message and the
tool result message) is sent back for the model to produce its final response.

## Streaming

SSE streaming is enabled with `"stream": true`. Chunks arrive as `data:` lines:

```text
data: {"choices":[{"delta":{"content":"Hello"},"index":0}]}
data: {"choices":[{"delta":{"content":" there"},"index":0}]}
data: [DONE]
```

For tool call argument streaming, additionally set `"tool_stream": true`. The
model progressively emits argument fragments as they are constructed.

## Thinking Mode

Enable deep reasoning with:

```json
{ "thinking": { "type": "enabled" } }
```

The model performs multi-step internal reasoning before responding. This:

- Increases response quality for complex or ambiguous queries
- Increases latency and token usage
- Has "preserved thinking" for coding — maintains reasoning continuity
  across turns
- Supports "turn-level thinking" for per-turn granular control

## JSON Structured Output

Force valid JSON responses with:

```json
{ "response_format": { "type": "json_object" } }
```

The system prompt should describe the expected schema. The model guarantees its
response is parseable JSON.

## OpenAI SDK Compatibility

GLM-5 is fully compatible with OpenAI's SDK. Existing projects can migrate by
updating `base_url` and `model`:

```bash
# cURL
curl https://api.z.ai/api/paas/v4/chat/completions \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5","messages":[{"role":"user","content":"Hello"}]}'
```
