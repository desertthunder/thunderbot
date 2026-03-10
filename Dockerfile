FROM rust:1.86-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p tnbot-cli

FROM debian:bookworm-slim AS runtime

RUN useradd --create-home --shell /usr/sbin/nologin tnbot
WORKDIR /home/tnbot
COPY --from=builder /app/target/release/tnbot /usr/local/bin/tnbot

USER tnbot
ENTRYPOINT ["tnbot"]
CMD ["--help"]
