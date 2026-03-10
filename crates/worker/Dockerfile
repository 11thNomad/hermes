FROM rust:1.94-bookworm AS chef
WORKDIR /workspace
RUN apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked \
    && cargo install cargo-watch --locked

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /workspace/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json --package worker
COPY . .
RUN cargo build --release -p worker

FROM chef AS dev
WORKDIR /workspace

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/worker /usr/local/bin/worker
CMD ["worker"]
