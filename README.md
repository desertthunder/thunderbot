# Thunderbot

Stateful AI agent that lives on Bluesky.

## Local Run

```bash
cargo run -p tnbot-cli -- config show
```

Configuration can be provided by:

- `.env` (for `TNBOT_*` vars)
- `tnbot.toml` in the working directory
- `--config /path/to/config.toml`

## Container Run

```bash
docker build -t thunderbot .
docker run --rm thunderbot --help
```
