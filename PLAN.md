# Revised Implementation Plan: Phased Build for the Private Video Streaming Service

## Summary
- Update the plan. Most review points are real gaps, and the implementation should be reorganized into testable phases rather than one flat build list.
- Keep the core architecture unchanged: anonymous upload, immediate raw playback, async HLS generation, SSE-first readiness updates, Redis Streams worker model.
- Refine a few internal interfaces now to remove ambiguity before coding:
  - use a DB trigger for `updated_at`
  - store `hls_manifest_key` instead of an ambiguous `hls_key`
  - keep `/api/videos/:id/status` as JSON and `/api/videos/:id/events` as SSE
  - make bucket creation idempotent and owned by shared startup bootstrap logic used by both API and worker

## Public / Internal Interface Decisions
- API routes:
  - `POST /api/videos` accepts multipart field `video`, returns `{ id, shareable_url }`
  - `GET /api/videos/:id/status` returns JSON status only
  - `GET /api/videos/:id/events` serves SSE only
  - `GET /api/videos/:id/stream` serves raw-file range requests
  - `GET /api/videos/:id/hls/index.m3u8` and `GET /api/videos/:id/hls/:segment` serve HLS assets once ready
- Database schema:
  - keep `videos` as the core table
  - add `detected_format TEXT NOT NULL`
  - add `attempt_count INT NOT NULL DEFAULT 0`
  - replace `hls_key` with `hls_manifest_key TEXT`
  - add a Postgres `BEFORE UPDATE` trigger to set `updated_at = now()`
- Object key layout:
  - raw upload: `{video_id}/original.{ext}`
  - HLS manifest: `{video_id}/hls/index.m3u8`
  - HLS segments: `{video_id}/hls/segment_%03d.ts`
- Upload implementation:
  - use a single S3-compatible `PutObject` stream, not S3 multipart upload
  - this is acceptable because max file size is 1 GB
  - enforce the size cap while streaming and do not rely on SDK defaults for buffering behavior
- CORS:
  - add `CORS_ALLOWED_ORIGINS` env var
  - dev default: `http://localhost:5173,http://127.0.0.1:5173`
  - no credentials support in v1

## Phased Implementation
### Phase 1: Scaffold and Tooling
- Create the Rust workspace, frontend app, Docker Compose files, `.env.example`, SQLx migration setup, `Makefile`, and `.pre-commit-config.yaml`.
- Add minimal startup binaries for API and worker plus a basic frontend shell so the repo boots cleanly.
- Add shared config loading and a single startup bootstrap module used by both API and worker.
- Bootstrap behavior:
  - ensure MinIO buckets exist with idempotent create-if-missing calls
  - ensure the Redis consumer group exists
  - run DB migrations before app startup in local/dev flow
- Exit criteria:
  - `cargo check --workspace` passes
  - frontend installs and builds
  - `docker compose up` starts all services cleanly
  - pre-commit hooks and Make targets are wired and documented

### Phase 2: Storage, Schema, and Raw Streaming Path
- Implement shared models, DB queries, object storage helpers, and range parsing utilities.
- Add the `videos` migration with `updated_at` trigger and the finalized schema fields.
- Implement `POST /api/videos` with:
  - `Content-Length` pre-check when present
  - magic-bytes validation from the first bytes of the upload stream
  - byte-counting stream cap during object upload
  - DB insert after successful object write
  - Redis enqueue after DB insert
- Implement `GET /api/videos/:id/stream` with proper `200`/`206`/`416` handling.
- Add a minimal upload/watch frontend that supports raw playback only.
- Exit criteria:
  - valid upload returns an ID and shareable URL
  - invalid format and oversized uploads are rejected
  - uploaded video is immediately watchable through the raw stream endpoint

### Phase 3: Worker, HLS Generation, and Status APIs
- Implement the worker consume loop with Redis Streams consumer groups.
- Processing flow:
  - claim or read a job
  - mark video `processing`
  - run `ffprobe` as validation gate
  - run deterministic `ffmpeg` HLS output:
    - single rendition
    - `index.m3u8`
    - `segment_%03d.ts`
    - 4-second segments
    - H.264 video + AAC audio
    - VOD playlist
  - upload manifest and segments to object storage
  - set `hls_manifest_key`, mark `ready`, ack the job
- Failure handling:
  - permanent media failures: mark `failed`, save `error_msg`, ack
  - transient infra failures: retry up to 3 total attempts with 5s and 15s backoffs
  - on final transient failure: write to `transcode_jobs_dlq`, mark `failed`, ack
  - reclaim pending messages older than `JOB_VISIBILITY_TIMEOUT_SECS`
- Implement status APIs:
  - `/status` returns JSON state and URLs
  - `/events` emits SSE `status` events plus keepalives
- Exit criteria:
  - a successful upload transitions to `ready` and serves HLS assets
  - permanent and transient failures produce the expected DB state and DLQ behavior
  - stalled jobs are reclaimed after timeout

### Phase 4: Frontend HLS Upgrade and UX Completion
- Upgrade the watch page to:
  - begin with raw playback immediately
  - subscribe to SSE on `/events`
  - switch to HLS when `status=ready`
  - preserve current time when switching if the player permits
  - fall back to polling `/status` if SSE fails
- Finalize upload UX with progress, success redirect, and failure states.
- Add explicit failed-transcode UI on the watch page.
- Exit criteria:
  - user sees upload progress
  - watch page starts playing before HLS is ready
  - player upgrades to HLS automatically without reload
  - fallback polling works if SSE is unavailable

### Phase 5: Documentation and Submission Hardening
- Write the final architecture document to match the implemented system, not just the original concept.
- Document accepted tradeoffs:
  - single-rendition HLS
  - no auth, expiry, resumable uploads, thumbnails, or signed URLs
  - API-proxied media delivery in v1
- Run the full local verification pass and prepare the repo for submission.

## Test Plan
- Phase 1:
  - workspace/build sanity
  - compose boot
  - bootstrap idempotency for bucket/group creation
- Phase 2:
  - magic-bytes detection
  - byte-cap enforcement
  - DB insert/query coverage
  - raw stream range handling
  - upload happy path and rejection paths
- Phase 3:
  - worker happy path
  - permanent ffprobe failure
  - transient retry path
  - DLQ write on retry exhaustion
  - pending-job reclaim
  - HLS endpoints only after `ready`
- Phase 4:
  - upload progress UI
  - SSE event handling
  - polling fallback
  - raw-to-HLS switch behavior
  - failed-job rendering
- Final smoke test:
  - upload one valid video through the UI
  - confirm immediate raw playback
  - confirm transition to HLS
  - confirm architecture doc matches actual behavior

## Assumptions And Defaults
- The `/status` and `/events` route split is intentional. It is a refinement of the spec to keep response types stable and implementation simpler.
- Bucket creation is performed by shared bootstrap code called by both API and worker startup. The create calls must be idempotent so concurrent first-run startup is safe.
- S3 multipart upload is out of scope for v1 because the hard cap is 1 GB. If later requirements exceed that, upload strategy should be revisited.
- HLS output is deterministic and single-rendition in v1; adaptive bitrate is explicitly deferred.
- DX tooling in the assignment spec is in scope and should be delivered in Phase 1, not left as optional polish.
