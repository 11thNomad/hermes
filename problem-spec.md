# Problem Spec: Minimal Private Video Streaming Service

> **Assignment source:** LabBase — Software X Discover Engineer take-home  
> **Time limit:** 1 week  
> **Evaluates:** Architectural thinking, code quality, communication skills

---

## 1. Original Requirements

### Goal
Design and implement a minimal private video streaming service. The main focus is on strong architecture design and a maintainable codebase. A functional UI is sufficient.

### Core Requirements
1. Users can upload video files
   - Upload size limited to **1GB**
   - Support common video formats (MP4, MKV, WebM, MOV)
   - Videos can be uploaded **anonymously** (no auth required)
   - System generates a **shareable link** per video
2. Users can stream videos by accessing the shareable link via the browser
3. **Time-to-stream** (time from upload start until streamable) is more important than high video quality
4. Provide a short architecture document explaining core decisions and overall design

### Bonus Requirements
1. Playback performance should feel consistent regardless of file size
2. The system should be able to scale horizontally
3. The architecture should be cost-efficient (cheap to run)

### Hard Constraints
- Preferred stack: **Rust** (backend) + **Svelte** (frontend)
- No authentication implementation required
- No IaC required
- Architecture and system design doc is a **hard requirement** even if not all code is complete
- AI tooling is permitted; collaboration with other people is not

### Submission
ZIP all files and submit via: https://forms.gle/6jZCyGmf3UyEBXAZ6

---

## 2. Stack Decisions

| Layer | Choice | Rationale |
|---|---|---|
| Backend HTTP API | Rust + **Axum** | Async-first, ergonomic tower middleware, strong ecosystem |
| Frontend | **SvelteKit** | Required, fast to build, SSR optional |
| Object storage | **MinIO** (Docker) | S3-compatible; swap to real S3 for production with zero code change |
| Database | **PostgreSQL** (Docker) | Reliable, supports UUID natively, good sqlx support |
| Job queue | **Redis Streams** (Docker) | Durable, supports consumer groups + ack, survives worker crashes |
| Transcoding | **ffmpeg** via subprocess | Industry standard; invoked by worker, not API |
| Video player | **hls.js** embedded in Svelte | HLS adaptive streaming in-browser |
| Migrations | **sqlx-cli** | Version-controlled, integrated with sqlx |

### Key Rust Crates
- `axum` — HTTP framework
- `sqlx` — async Postgres, compile-time query checking
- `aws-sdk-s3` or `object_store` — MinIO / S3 abstraction
- `redis` (with `tokio` feature) — Redis Streams job queue
- `uuid` — shareable link ID generation
- `tokio` — async runtime (API + worker)
- `tracing` + `tracing-subscriber` — structured logging
- `serde` + `serde_json` — serialization
- `thiserror` — error types
- `tower-http` — CORS, request size limits, tracing middleware

---

## 3. Architecture

### 3.1 System Diagram

```
┌─────────────────────────────────────────────────────┐
│                   Docker Compose                     │
│                                                      │
│  ┌──────────┐    ┌──────────┐    ┌───────────────┐  │
│  │  Svelte  │───▶│  Axum   │───▶│     Redis     │  │
│  │  (5173)  │    │  API    │    │  Streams      │  │
│  └──────────┘    │  (8080) │    └───────┬───────┘  │
│                  └────┬────┘            │           │
│                       │          ┌──────▼───────┐   │
│                       │          │   Worker(s)  │   │
│                       │          │  (ffmpeg +   │   │
│                       │          │  transcoder) │   │
│                       │          └──────┬───────┘   │
│                  ┌────▼────┐            │           │
│                  │Postgres │     ┌──────▼───────┐   │
│                  │  (5432) │     │    MinIO     │   │
│                  └─────────┘     │   (9000)     │   │
│                                  └──────────────┘   │
└─────────────────────────────────────────────────────┘
```

### 3.2 Upload & Stream Flow

```
1. Client sends multipart upload (≤1GB) to POST /api/videos
2. Axum middleware:
     a. Checks Content-Length ≤ 1GB — reject early if over
     b. Reads first 12 bytes — magic bytes validation against allowlist
     c. Streams remainder directly to MinIO (never fully buffered in memory)
3. API writes video record to Postgres (status: pending)
4. API enqueues job to Redis Stream with video_id + raw MinIO key
5. API immediately returns { id, shareable_url } to client
6. Client navigates to /watch/:id
     → Player begins streaming original file via HTTP range requests (206)
     → This satisfies time-to-stream: streamable within seconds of upload completing
7. Worker picks up Redis job:
     a. Pulls raw file from MinIO
     b. Runs ffprobe to re-validate file (second security gate)
     c. Runs ffmpeg to transcode to HLS segments (.m3u8 + .ts files)
     d. Writes HLS output back to MinIO
     e. Updates Postgres status: ready, sets hls_key
     f. Acks Redis job
8. Frontend polls GET /api/videos/:id/status (or uses SSE)
     → Once status = ready, player switches from raw range stream → HLS
     → HLS gives consistent playback, better seeking regardless of file size
```

### 3.3 Dual-Path Serving Strategy

This is the core architectural decision for time-to-stream:

- **Immediate path:** Serve original uploaded file via range requests as soon as upload completes. No waiting for transcoding.
- **Optimised path:** Once worker finishes HLS transcoding, switch player to segmented HLS stream. This gives consistent buffer performance, adaptive bitrate capability, and better seeking on large files.

The player detects HLS readiness via a status poll or SSE event and upgrades transparently.

### 3.4 Security & Validation

Do **not** trust the `Content-Type` header — it is user-controlled. Validation layers:

1. **Content-Length check** — reject requests over 1GB before reading body
2. **Magic bytes check** (Axum middleware, first 12 bytes):

   | Format | Magic bytes |
   |--------|------------|
   | MP4    | `ftyp` at offset 4 |
   | MKV/WebM | `0x1A 0x45 0xDF 0xA3` |
   | MOV    | `ftyp` or `moov` |

3. **Stream cap** — enforce size limit mid-stream during MinIO write, abort if exceeded
4. **ffprobe re-validation** — worker validates file before handing to ffmpeg; malformed or malicious files are caught here and job is marked `failed`

### 3.5 Database Schema

```sql
CREATE TABLE videos (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  filename    TEXT NOT NULL,
  mime_type   TEXT NOT NULL,
  size_bytes  BIGINT NOT NULL,
  status      TEXT NOT NULL DEFAULT 'pending',
  -- status enum: pending | processing | ready | failed
  raw_key     TEXT NOT NULL,     -- MinIO object key for original file
  hls_key     TEXT,              -- MinIO key prefix for HLS; set when status = ready
  error_msg   TEXT,              -- populated on failure
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX ON videos (id);
```

### 3.6 Redis Job Schema

Jobs are pushed to a Redis Stream named `transcode_jobs`. Payload:

```json
{
  "video_id": "uuid",
  "source_bucket": "videos-raw",
  "source_key": "uuid/original.mp4",
  "output_bucket": "videos-hls",
  "output_prefix": "uuid/hls/"
}
```

Workers use a consumer group (`workers`) so each job is processed by exactly one worker. On crash/timeout, unacknowledged jobs are re-claimed by another worker after a visibility timeout (e.g. 5 minutes).

### 3.7 Horizontal Scaling

- **API containers** are stateless — scale freely behind a load balancer
- **Worker containers** are stateless consumers — `docker compose up --scale worker=N`
- **MinIO** is the shared storage backend; in production, replace with S3 (zero code change via `object_store` abstraction)
- **Redis Streams** consumer groups handle work distribution across N workers automatically
- **Postgres** is the single stateful component; use read replicas for status queries at scale

### 3.8 Cost Efficiency Notes

- MinIO on a cheap VPS replaces S3 for self-hosting
- Workers only do CPU work during transcoding; idle workers consume minimal resources
- In production: put a CDN (e.g. Cloudflare) in front of MinIO HLS delivery — cache `.ts` segments at edge, dramatically reduce origin bandwidth
- HLS segments are individually cacheable (immutable once written), making CDN integration highly effective

---

## 4. Monorepo Structure

```
/
├── docker-compose.yml              # production-like compose
├── docker-compose.dev.yml          # dev overrides (hot reload, ports exposed)
├── .env.example                    # all required env vars documented
├── .pre-commit-config.yaml         # lint + fmt hooks
├── Makefile                        # DX shortcuts
├── Cargo.toml                      # workspace root
├── Cargo.lock
│
├── crates/
│   ├── common/                     # shared crate: MinIO client, job types, DB types
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── storage.rs          # MinIO abstraction (object_store)
│   │       ├── queue.rs            # Redis Stream helpers
│   │       └── models.rs           # Video struct, status enum, job payload
│   │
│   ├── api/                        # Axum HTTP API binary
│   │   ├── Cargo.toml
│   │   ├── Dockerfile
│   │   └── src/
│   │       ├── main.rs
│   │       ├── routes/
│   │       │   ├── mod.rs
│   │       │   ├── upload.rs       # POST /api/videos
│   │       │   ├── stream.rs       # GET /api/videos/:id/stream (range requests)
│   │       │   └── status.rs       # GET /api/videos/:id/status (SSE or poll)
│   │       ├── middleware/
│   │       │   └── magic_bytes.rs  # format validation middleware
│   │       └── state.rs            # AppState (db pool, minio client, redis)
│   │
│   └── worker/                     # transcoding worker binary
│       ├── Cargo.toml
│       ├── Dockerfile
│       └── src/
│           ├── main.rs             # Redis Stream consumer loop
│           └── transcode.rs        # ffprobe validation + ffmpeg HLS pipeline
│
├── frontend/                       # SvelteKit app
│   ├── Dockerfile
│   ├── package.json
│   ├── svelte.config.js
│   ├── vite.config.js
│   └── src/
│       ├── routes/
│       │   ├── +page.svelte        # upload UI with progress bar
│       │   └── watch/
│       │       └── [id]/
│       │           └── +page.svelte  # video player page
│       └── lib/
│           ├── player/
│           │   └── HlsPlayer.svelte  # hls.js wrapper component
│           └── api.ts              # typed API client
│
└── docs/
    └── architecture.md             # the required architecture doc for submission
```

---

## 5. Docker Compose Services

```yaml
services:
  api:
    build: ./crates/api
    ports: ["8080:8080"]
    depends_on: [postgres, redis, minio]
    env_file: .env

  worker:
    build: ./crates/worker
    depends_on: [redis, minio, postgres]
    env_file: .env
    # scale with: docker compose up --scale worker=3

  frontend:
    build: ./frontend
    ports: ["5173:5173"]
    depends_on: [api]

  postgres:
    image: postgres:16-alpine
    volumes: [pgdata:/var/lib/postgresql/data]
    environment:
      POSTGRES_DB: streamr
      POSTGRES_USER: streamr
      POSTGRES_PASSWORD: streamr

  redis:
    image: redis:7-alpine
    volumes: [redisdata:/data]
    command: redis-server --appendonly yes  # persistence on

  minio:
    image: minio/minio
    command: server /data --console-address ":9001"
    ports: ["9000:9000", "9001:9001"]
    volumes: [miniodata:/data]
    environment:
      MINIO_ROOT_USER: minioadmin
      MINIO_ROOT_PASSWORD: minioadmin

volumes:
  pgdata:
  redisdata:
  miniodata:
```

---

## 6. DX Tooling

### Pre-commit Hooks (`.pre-commit-config.yaml`)
- `cargo fmt --check` — Rust formatting
- `cargo clippy -- -D warnings` — Rust linting (fail on warnings)
- `cargo test --workspace` — unit tests (fast only, no integration)
- `eslint` + `prettier` — Svelte/TS formatting and linting

### Makefile Targets
```makefile
dev           # docker compose -f docker-compose.yml -f docker-compose.dev.yml up
test          # cargo test --workspace && cd frontend && npm run test
lint          # cargo clippy && cd frontend && npm run lint
fmt           # cargo fmt && cd frontend && npm run format
db-migrate    # sqlx migrate run
db-seed       # insert test video records
logs          # docker compose logs -f worker
scale-workers # docker compose up --scale worker=3 -d
build         # docker compose build
clean         # docker compose down -v
```

### Dev Hot Reload
- Rust: `cargo-watch` in `docker-compose.dev.yml` override (`cargo watch -x run`)
- Svelte: Vite HMR works natively in dev mode

### Testing Strategy
- **Unit tests** (in-crate, fast): magic bytes validator, job payload serialization, status state machine transitions
- **Integration tests** (separate `tests/` dir, requires Docker): upload → enqueue → worker → status flow
- **Frontend**: Vitest for component logic, Playwright for E2E (optional/stretch)

---

## 7. Environment Variables

```env
# API
DATABASE_URL=postgres://streamr:streamr@postgres:5432/streamr
REDIS_URL=redis://redis:6379
MINIO_ENDPOINT=http://minio:9000
MINIO_ACCESS_KEY=minioadmin
MINIO_SECRET_KEY=minioadmin
MINIO_RAW_BUCKET=videos-raw
MINIO_HLS_BUCKET=videos-hls
MAX_UPLOAD_BYTES=1073741824   # 1GB
BASE_URL=http://localhost:8080

# Worker (same storage/db vars as above)
TRANSCODE_STREAM=transcode_jobs
TRANSCODE_CONSUMER_GROUP=workers
JOB_VISIBILITY_TIMEOUT_SECS=300
```

---

## 8. API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/videos` | Multipart upload. Returns `{ id, url }` |
| `GET` | `/api/videos/:id/status` | Poll or SSE for transcode status |
| `GET` | `/api/videos/:id/stream` | Range-request streaming of raw file |
| `GET` | `/api/videos/:id/hls/index.m3u8` | HLS manifest (available once status=ready) |
| `GET` | `/api/videos/:id/hls/:segment` | HLS segment delivery |

---

## 9. What to Build vs. Describe

| Component | Approach |
|-----------|----------|
| Axum upload endpoint + range streaming | **Build** |
| Magic bytes middleware | **Build** |
| Postgres schema + sqlx queries | **Build** |
| Redis Stream enqueue (API side) | **Build** |
| Worker consumer loop | **Build** |
| ffmpeg HLS transcoding pipeline | **Build** |
| Svelte upload page + progress bar | **Build** |
| Svelte player page + hls.js | **Build** |
| SSE status endpoint | **Build** (or poll fallback) |
| CDN integration | **Describe in architecture doc** |
| IaC / deployment | **Not required** |
| Authentication | **Not required** |

---

## 10. Architecture Doc Outline (for submission)

The `docs/architecture.md` file (required for submission) should cover:

1. **Overview** — what the system does, key constraints
2. **Dual-path serving decision** — why immediate raw streaming + async HLS satisfies time-to-stream
3. **Queue architecture** — why Redis Streams over in-process Tokio tasks (durability, independent scaling)
4. **Security model** — magic bytes + ffprobe two-gate validation
5. **Scaling model** — stateless API, stateless workers, shared MinIO, Redis consumer groups
6. **Cost efficiency** — MinIO self-hosting, CDN for HLS segment caching
7. **Tradeoffs accepted** — no adaptive bitrate on immediate stream, no deduplication, no expiry on links
