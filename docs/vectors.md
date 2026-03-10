# Vector Memory & Semantic Search

Extend the agent's memory beyond per-thread context. Embed conversation
fragments and enable semantic retrieval across threads so the agent can recall
relevant past interactions when composing responses.

## Vector Storage (libSQL Native)

libSQL has **built-in** vector search. This avoids adding the `sqlite-vec` extension as a dependency:

| Feature                 | libSQL Native               | sqlite-vec Extension           |
| ----------------------- | --------------------------- | ------------------------------ |
| Installation            | Already bundled             | Requires C extension loading   |
| Column type             | `F32_BLOB(N)`               | `vec0` virtual table           |
| Distance function       | `vector_distance_cos(a, b)` | `vec_distance_cosine(a, b)`    |
| KNN function            | `vector_top_k(idx, q, k)`   | `WHERE embedding MATCH q`      |
| Index                   | `libsql_vector_idx`         | Implicit in `vec0`             |
| Mixed relational+vector | Same table                  | Separate virtual table + JOINs |

## Embedding Generation

Z.ai does not expose a dedicated embedding endpoint.
We use a local embedding model via **Ollama** to keep costs at zero and latency low.
All models below are available via `ollama pull`:

| Model                        | Params | Dims          | Context  | Notes                                         |
| ---------------------------- | ------ | ------------- | -------- | --------------------------------------------- |
| **`embeddinggemma`**         | 308M   | 768 (MRL→128) | 2K tok   | Google, #1 open multilingual <500M on MTEB    |
| `nomic-embed-text` v1.5      | 137M   | 768 (MRL→64)  | 8K tok   | Excellent long-context, beats OAI ada-002     |
| `mxbai-embed-large`          | 335M   | 1024          | 512 tok  | SOTA for BERT-large class, great all-rounder  |
| `snowflake-arctic-embed2`    | <1B    | 1024 (MRL)    | 8K tok   | Enterprise-grade, multilingual                |
| `qwen3-embedding`            | 8B     | 4096 (→32)    | 32K tok  | #1 MTEB multilingual, heavy                   |
| `all-MiniLM-L6-v2`           | 22M    | 384           | 256 tok  | Ultra-lightweight, sentence-level only        |

Most of these support **Matryoshka Representation Learning (MRL)**, which means
the model is trained so that the first N dimensions of the vector are independently useful.
You can truncate a 768-dim vector to 256 or 128 dims at query time and still get reasonable quality.
This is valuable if we ever need to optimize storage or speed without re-embedding everything.

`embeddinggemma` is the primary model:

- Runs on <200MB RAM (quantized) — perfect for self-hosted bots
- 100+ languages out of the box (matches Bluesky's global userbase)
- 768 dims at default, truncatable to 256 or 128 via MRL
- Best-in-class quality for its size class
- Available via `ollama pull embeddinggemma`

`nomic-embed-text` v1.5 is the fallback if longer context (8K vs 2K) is needed for
embedding multi-post thread summaries.

Embeddings are stored at `F32_BLOB(768)` to match EmbeddingGemma's native output.
If we later switch to a higher-dimensionality model, a migration can resize the column.

### Embedding Pipeline

```text
New conversation stored
  → Check embedding_jobs for conversation_id
  → If missing → INSERT with status = 'pending'
  → Background worker picks up pending jobs:
      1. Load content from conversations table
      2. Call EmbeddingProvider::embed(content)
      3. INSERT INTO memories (embedding = vec_f32(?))
      4. UPDATE embedding_jobs SET status = 'complete'
      5. On failure: increment attempts, retry up to 3×
```

## Building Prompts

Retrieved memories are injected as a second system message, after the
constitution but before the thread history. This gives the model background
context about past interactions without conflating it with the current
conversation. The model sees something like:

> **System**: [constitution]
> **System**: Relevant context from past conversations:
> [Memory from 2026-03-01]: User asked about Rust async patterns...
> [Memory from 2026-02-15]: Discussion about AT Protocol threading...
> **User**: [@alice.bsky.social]: Hey bot, how does threading work again?

## Memories

Over time, individual post embeddings accumulate. A 50-message thread produces 50 memories,
many of which are semantically redundant ("hi", "thanks", "yes").
Consolidation compresses a completed thread into a single summary memory that captures the gist of what was discussed.

After a thread has been inactive for 24 hours (no new posts), we:

1. Load the full thread from the database.
2. Send it to GLM-5 with a summarization prompt.
3. Embed the summary via EmbeddingGemma.
4. Store the summary as a new consolidated memory.
5. Delete the individual post memories for that thread.

### Deduplication

If the bot is mentioned in a thread multiple times, the same content might be embedded more than once.
Before inserting a new memory, we check the cosine distance to existing memories for the same `root_uri`.
If any existing memory has a distance < 0.05 (nearly identical), we skip the insertion.
This prevents redundant embeddings without incurring significant query overhead (the check is a single KNN lookup scoped to one thread).
