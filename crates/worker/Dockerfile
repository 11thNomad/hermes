FROM rust:1.94-bookworm AS builder
WORKDIR /workspace

COPY . .
RUN cargo build --release -p worker

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/worker /usr/local/bin/worker
CMD ["worker"]
