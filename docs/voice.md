# Audio Pipeline for Thunderbot

Research on local speech-to-text (whisper.cpp) and text-to-speech (piper) for voice interaction.

## Speech-to-Text: whisper.cpp

OpenAI Whisper reimplemented in plain C/C++. No Python, no dependencies. Runs entirely on-device.

| Property    | Value                                                                               |
| ----------- | ----------------------------------------------------------------------------------- |
| Repo        | [ggml-org/whisper.cpp](https://github.com/ggml-org/whisper.cpp)                     |
| Version     | v1.8.0 (2026)                                                                       |
| License     | MIT                                                                                 |
| Rust Crate  | [`whisper-rs`](https://crates.io/crates/whisper-rs) (Metal, CUDA, Vulkan, OpenBLAS) |
| HTTP Server | Built-in `whisper-server` binary with OpenAI-compatible API                         |

### Models

| Model                 | Params | Disk    | RAM     | English WER | Notes                             |
| --------------------- | ------ | ------- | ------- | ----------- | --------------------------------- |
| `ggml-tiny`           | 39M    | ~75 MB  | ~390 MB | ~8%         | Fastest, lowest quality           |
| `ggml-base`           | 74M    | ~142 MB | ~500 MB | ~6%         | Good speed/quality tradeoff       |
| **`ggml-small`**      | 244M   | ~466 MB | ~1 GB   | ~4%         | **Recommended starting point**    |
| _`ggml-medium`_       | 769M   | ~1.5 GB | ~2.6 GB | ~3%         | Better accuracy, slower           |
| `ggml-large-v3`       | 1.5B   | ~3.1 GB | ~4.7 GB | ~2%         | Best accuracy                     |
| `ggml-large-v3-turbo` | 809M   | ~1.6 GB | ~2.8 GB | ~2.5%       | Near large-v3 accuracy, 2× faster |

All models support integer quantization (Q4, Q5, Q8) for further size/RAM reduction.

### HTTP Server

```bash
# Start the server
whisper-server --model models/ggml-small.bin --host 127.0.0.1 --port 8081

# Transcribe audio (OpenAI-compatible)
curl http://127.0.0.1:8081/v1/audio/transcriptions \
  -F "file=@audio.wav" \
  -F "model=whisper-1"
```

The server exposes:

- `POST /v1/audio/transcriptions` — speech to text
- `POST /v1/audio/translations` — speech to English text

Supported audio: WAV, MP3, FLAC, OGG, AAC, M4A.

### Rust Integration (`whisper-rs`)

```rust
// Cargo.toml
// whisper-rs = { version = "0.13", features = ["metal"] }  // or "cuda"

use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};

let ctx = WhisperContext::new_with_params(
    "models/ggml-small.bin",
    WhisperContextParameters::default(),
)?;
let mut state = ctx.create_state()?;

let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
params.set_language(Some("en"));

state.full(params, &pcm_data)?;

let text = state.full_get_segment_text(0)?;
```

### Performance (Apple Silicon, M-series)

| Model          | Real-time Factor | Latency (10s clip) |
| -------------- | ---------------- | ------------------ |
| small          | ~0.3×            | ~3s                |
| medium         | ~0.8×            | ~8s                |
| large-v3-turbo | ~1.0×            | ~10s               |

Metal acceleration is automatic on macOS.

## Text-to-Speech: Piper

Fast, local neural TTS. VITS-based models exported to ONNX Runtime. No cloud, no API keys.

| Property    | Value                                                                                                        |
| ----------- | ------------------------------------------------------------------------------------------------------------ |
| Repo        | [rhasspy/piper](https://github.com/rhasspy/piper)                                                            |
| License     | MIT                                                                                                          |
| Rust Crate  | [`piper-rs`](https://github.com/mush42/piper-rs) — pure Rust, all models supported                           |
| Alt Crate   | [`piper-tts-rs`](https://crates.io/crates/piper-tts-rs) — raw FFI bindings                                   |
| HTTP Server | [`TTS-API-Server`](https://github.com/manmay-nakhashi/TTS-API-Server) — OpenAI `/v1/audio/speech` compatible |
| Streaming   | Sentence-boundary streaming (late 2025+)                                                                     |

### Voice Quality Tiers

| Quality    | Sample Rate | Speed    | Model Size | Notes                                 |
| ---------- | ----------- | -------- | ---------- | ------------------------------------- |
| `x_low`    | 16 kHz      | Fastest  | ~15 MB     | Robotic, only for constrained devices |
| `low`      | 16 kHz      | Fast     | ~30 MB     | Acceptable for notifications          |
| `medium`   | 22.05 kHz   | Moderate | ~50 MB     | Decent for general use                |
| **`high`** | 22.05 kHz   | Moderate | ~60 MB     | **Recommended** — natural sounding    |

### Recommended English Voices

| Voice ID            | Quality | Notes                               |
| ------------------- | ------- | ----------------------------------- |
| `en_US-ryan-high`   | High    | Male, clear, good for assistant use |
| `en_US-amy-medium`  | Medium  | Female, natural cadence             |
| `en_US-lessac-high` | High    | Male, expressive                    |
| `en_GB-alan-medium` | Medium  | British male                        |

Full voice list: [rhasspy/piper/releases](https://github.com/rhasspy/piper/releases)

## Integration Architecture

```text
                    ┌──────────────┐
   Microphone ────▶ │ whisper.cpp  │ ────▶ Text (STT)
                    │ (whisper-rs) │               │
                    └──────────────┘               ▼
                                          ┌──────────────┐
                                          │  Thunderbot  │
                                          │  AI Pipeline │
                                          └──────┬───────┘
                                                 │
                                                 ▼
                    ┌──────────────┐        Response Text
   Speaker ◀─────   │    Piper     │ ◀───────────┘
                    │  (piper-rs)  │
                    └──────────────┘
```
