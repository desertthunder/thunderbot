# Model Options

## Cloud: Kimi K2.5 (Moonshot AI)

| Property          | Value                                                                     |
| ----------------- | ------------------------------------------------------------------------- |
| Architecture      | MoE — 1T total params, **32B active** per token (384 experts, 8 selected) |
| Context Window    | **262,144 tokens**                                                        |
| Multimodal        | Native (text + vision via MoonViT 400M)                                   |
| API Compatibility | **OpenAI-compatible** — drop-in replacement                               |
| Base URL          | `https://api.moonshot.ai/v1` (CN: `https://api.moonshot.cn/v1`)           |
| Model ID          | `kimi-k2.5`                                                               |
| Input Price       | $0.60 / M tokens ($0.10 on cache hit)                                     |
| Output Price      | $3.00 / M tokens                                                          |
| Modes             | Instant, Thinking, Agent, Agent Swarm                                     |
| Open Source       | Yes (weights on HuggingFace)                                              |

## Local Models (via Ollama)

All run through Ollama's OpenAI-compatible endpoint at `http://localhost:11434/v1`.

### Gemma 3 4B

| Property       | Value            |
| -------------- | ---------------- |
| Parameters     | 4B dense         |
| Quantized Size | ~2.5 GB (Q4_K_M) |
| RAM Required   | ~4 GB            |
| Context        | 32K              |
| Ollama Tag     | `gemma3:4b`      |

- Google model — fast and efficient
- Good for general chat, analysis, light coding
- Fits comfortably on machines with limited RAM
- Already aligned with our embedding choice (EmbeddingGemma)

### Mistral 7B

| Property       | Value            |
| -------------- | ---------------- |
| Parameters     | 7B dense         |
| Quantized Size | ~4.1 GB (Q4_K_M) |
| RAM Required   | ~6 GB            |
| Context        | 32K              |
| Ollama Tag     | `mistral:7b`     |

- Fast, reliable workhorse
- Clean instruction following
- Good for summaries, conversation, and general tasks
- Slightly weaker on reasoning than Qwen3

## Summary

| Scenario                    | Model      | Notes                                                  |
| --------------------------- | ---------- | ------------------------------------------------------ |
| **Production (cloud)**      | Kimi K2.5  | Best bang-for-buck, massive context, OpenAI-compatible |
| **Local dev (lightweight)** | Gemma 3 4B | Low RAM, fast, aligned with our embedding stack        |
| **Local dev (capable)**     | Mistral 7B | Stronger instruction following, more headroom          |
