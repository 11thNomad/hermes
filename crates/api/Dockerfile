FROM rust:1.94-bookworm AS builder
WORKDIR /workspace

COPY . .
RUN cargo build --release -p api

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/api /usr/local/bin/api
CMD ["api"]
