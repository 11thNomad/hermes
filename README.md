# Hermes

Hermes is a phased implementation of a private video upload and streaming service using Rust, SvelteKit, PostgreSQL, Redis Streams, and MinIO.

## Status

Phase 1 is complete:
- Rust workspace and shared bootstrap flow
- Docker Compose stack for API, worker, frontend, Postgres, Redis, and MinIO
- Frontend shell and health endpoints
- Phase 1 regression tests and smoke checks

Phases 2 through 5 are still pending.

## Prerequisites

- Rust toolchain with `cargo`, `rustfmt`, and `clippy`
- Node.js and npm
- Docker with `docker compose`
- Python 3 if you want to install `pre-commit`

## Setup

1. Copy the environment file:
```bash
cp .env.example .env
```

2. Install frontend dependencies:
```bash
make bootstrap
```

3. Optional: install and enable pre-commit hooks:
```bash
python3 -m pip install --user pre-commit
export PATH="$HOME/.local/bin:$PATH"
pre-commit install
```

If `pre-commit` is not found after the pip install, make sure `~/.local/bin` is on `PATH`.

## Run

Development compose stack:
```bash
make dev
```

Production-style image build:
```bash
make build
docker compose up -d
```

## Validation

Workspace checks:
```bash
make check
make lint
make test
```

Phase 1 smoke test:
```bash
make smoke-phase1
```

## Useful Endpoints

- API health: `http://localhost:8080/healthz`
- API health alias: `http://localhost:8080/api/healthz`
- API root info: `http://localhost:8080/`
- Frontend shell: `http://localhost:4173`
- MinIO API: `http://localhost:9000`
- MinIO console: `http://localhost:9001`

## Logs

Follow app logs:
```bash
make logs
```

Or inspect the full compose stack:
```bash
docker compose logs -f
```

The API and worker now emit useful startup and bootstrap logs, including:
- migration execution
- MinIO bucket checks/creation
- Redis consumer-group checks/creation
- API request traces
- worker bootstrap and heartbeat activity

## Notes

- Both the API and worker use shared bootstrap code in `crates/common`.
- Startup bootstrap is idempotent: it runs migrations, ensures MinIO buckets exist, and ensures the Redis consumer group exists.
- The current API only exposes health/info routes. Upload, raw streaming, HLS generation, and watch behavior arrive in Phase 2 and later.
