# Hermes

Hermes is a phased implementation of a private video upload and streaming service using Rust, SvelteKit, PostgreSQL, Redis Streams, and MinIO.

## Status

Phase 1 is complete:
- Rust workspace and shared bootstrap flow
- Docker Compose stack for API, worker, frontend, Postgres, Redis, and MinIO
- Phase 1 regression tests and smoke checks

Phase 2 is complete:
- `videos` schema and migration
- multipart upload endpoint with size checks and magic-bytes validation
- raw object storage in MinIO
- DB insert and Redis enqueue after successful upload
- raw stream endpoint with `200` / `206` / `416` range handling
- minimal upload and watch UI
- generated OpenAPI spec and Swagger UI

Phases 3 through 5 are still pending.

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

`make dev` now matches the recommended local workflow:
- `api` and `worker` run through `cargo-watch` with polling inside the dev containers
- Postgres, Redis, and MinIO stay in Docker
- the frontend is intended to run on the host for the most reliable Vite hot reload

Run the frontend in a second terminal:
```bash
make frontend-dev
```

That gives you:
- backend and infra in Docker
- frontend Vite dev server on the host at `http://localhost:5173`

If you want the previous all-Docker dev flow, it is still available:
```bash
make dev-full
```

Production-style image build:
```bash
make build
docker compose up -d
```

If you change the API and need a fresh container image:
```bash
docker compose up -d --build api
```

The production-style Rust Dockerfiles now use `cargo-chef`, so repeated `docker compose build` runs should be much faster after the first dependency build.

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

Phase 2 upload and stream smoke test:
```bash
make smoke-upload
```

Phase 2 checks are covered by the normal workspace commands:
```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd frontend && npm run lint && npm run build
```

## Useful Endpoints

- API health: `http://localhost:8080/healthz`
- API health alias: `http://localhost:8080/api/healthz`
- API root info: `http://localhost:8080/`
- Swagger UI: `http://localhost:8080/swagger-ui/`
- OpenAPI JSON: `http://localhost:8080/api-doc/openapi.json`
- Frontend upload page: `http://localhost:4173`
- MinIO API: `http://localhost:9000`
- MinIO console: `http://localhost:9001`

## API Surface

- `POST /api/videos`
  - accepts multipart form field `video`
  - returns `{ id, shareable_url }`
- `GET /api/videos/:id/stream`
  - serves the raw uploaded object
  - supports HTTP range requests
- `GET /healthz`
- `GET /api/healthz`
- `GET /`

Example upload:
```bash
curl -F "video=@/path/to/video.mp4" http://localhost:8080/api/videos
```

Example range request:
```bash
curl -H "Range: bytes=0-1023" -i http://localhost:8080/api/videos/<id>/stream
```

## Logs

Follow app logs:
```bash
make logs
```

Or inspect the full compose stack:
```bash
docker compose logs -f
```

When using the recommended dev flow, `make logs` follows only the API and worker containers because the frontend is running on the host.

The API and worker now emit useful startup and bootstrap logs, including:
- migration execution
- MinIO bucket checks/creation
- Redis consumer-group checks/creation
- API request traces
- worker bootstrap and heartbeat activity
- upload and raw streaming activity

`make smoke-upload` now verifies:
- sample upload from `test-videos/drums.webm`
- `201 Created` response with returned `id`
- full raw stream request returning `200`
- ranged raw stream request returning `206` with `Accept-Ranges` and `Content-Range`

## Notes

- Both the API and worker use shared bootstrap code in `crates/common`.
- Startup bootstrap is idempotent: it runs migrations, ensures MinIO buckets exist, and ensures the Redis consumer group exists.
- Phase 2 currently serves raw playback only. HLS generation, status APIs, SSE updates, and automatic raw-to-HLS switching are Phase 3 and 4 work.
- Swagger UI is generated from `utoipa` annotations in the API crate. If `/swagger-ui/` returns `404`, rebuild the API container so the running image matches the current source.
