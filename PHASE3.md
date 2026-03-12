# Phase 3 Plan

Phase 3 is where Hermes moves from "raw upload and playback" to "asynchronous processing pipeline."

The goal is to keep implementation incremental and testable. The main mistake to avoid is landing queue consumption, DB state transitions, `ffprobe`, `ffmpeg`, retries, DLQ handling, status JSON, and SSE in one large change.

## Scope

Phase 3 should deliver:
- worker consumption from Redis Streams
- DB status transitions for uploaded videos
- media validation with `ffprobe`
- HLS generation with `ffmpeg`
- HLS asset upload to MinIO
- status JSON endpoint
- SSE status events endpoint
- retry / reclaim / DLQ behavior

## Recommended Sequence

### 1. Worker Job Lifecycle Only

Implement:
- read from the Redis Streams consumer group
- claim one job
- deserialize the payload
- log the `video_id`
- ack it manually or otherwise mark it consumed
- no `ffmpeg` yet

Exit criterion:
- uploaded jobs are visibly consumed by the worker

### 2. DB Status Mutation Layer

Implement shared DB helpers for:
- set `processing`
- set `ready` with `hls_manifest_key`
- set `failed` with `error_msg`
- increment `attempt_count`

Exit criterion:
- worker can move a real video row from `pending` to `processing`

### 3. Fake Processor

Replace "just log the job" with:
- mark `processing`
- sleep briefly
- mark `ready`
- set a fake `hls_manifest_key`
- ack job

Exit criterion:
- end-to-end queue and status flow works without media tooling

### 4. Status API

Implement:
- `GET /api/videos/:id/status`

Return:
- current status
- raw stream URL
- HLS URL when ready

Exit criterion:
- after upload, status can be polled and observed moving through states

### 5. SSE Events

Implement:
- `GET /api/videos/:id/events`

Behavior:
- emit status events
- emit keepalives

Exit criterion:
- a client can subscribe and receive transitions in real time

### 6. Real Media Validation

Implement:
- `ffprobe` validation inside the worker

Behavior:
- invalid media is a permanent failure
- mark `failed`
- persist `error_msg`
- ack the job

Exit criterion:
- bad media fails cleanly without crashing the worker

### 7. Real HLS Generation

Implement deterministic single-rendition `ffmpeg` output:
- manifest: `index.m3u8`
- segments: `segment_%03d.ts`
- VOD playlist
- H.264 video + AAC audio
- single rendition only

Then:
- upload generated HLS assets to MinIO
- persist real `hls_manifest_key`
- mark `ready`
- ack the job

Exit criterion:
- uploaded file becomes HLS-ready and assets exist in MinIO

### 8. Retry and Reclaim Logic

Implement:
- retry for transient failures
- increment `attempt_count`
- reclaim stuck pending messages older than the visibility timeout
- send exhausted jobs to a DLQ

Exit criterion:
- worker survives interruptions and transient infra failures

## Why This Order

- It separates queue and state-machine correctness from media-processing complexity.
- It gives clean, debuggable checkpoints.
- It avoids mixing worker reliability bugs with `ffmpeg` bugs.
- It makes Phase 3 reviewable in small pieces.

## Main Risks

- representing retries cleanly in the DB
- distinguishing permanent media failures from transient infra failures
- reclaiming stuck Redis pending messages safely
- getting `ffmpeg` to run reproducibly inside the worker container

## Testing Strategy

### Steps 1-3
- unit / integration tests around queue payloads
- DB status helper coverage
- worker lifecycle smoke without real transcoding

### Steps 4-5
- API tests for `/status`
- API tests for `/events`
- SSE contract checks

### Steps 6-7
- worker integration smoke with one known-good sample
- worker integration smoke with one known-bad sample

### Step 8
- retry count tests
- DLQ write tests
- pending-message reclaim tests

## Immediate Next Sub-Phase

The best next implementation block is Steps 1-3 together:
- worker consume loop
- DB mutation helpers
- fake processor

This gives Hermes a full asynchronous state machine before `ffmpeg` enters the picture.

## Concrete Checklist

### Worker Consume Loop
- Add shared queue helper for reading one message from the consumer group.
- Add shared queue helper for acknowledging one message.
- Update worker main loop to:
  - read one job
  - deserialize payload
  - log message id and `video_id`
- Decide consumer naming strategy for local/dev runs.
- Confirm uploaded jobs appear in worker logs.

### DB Mutation Helpers
- Add shared DB helper to set video status to `processing`.
- Add shared DB helper to set video status to `ready`.
- Add shared DB helper to set video status to `failed`.
- Add shared DB helper to update `hls_manifest_key`.
- Add shared DB helper to increment `attempt_count`.
- Add tests for each mutation helper.

### Fake Processor
- In worker, after reading a job:
  - load the target video row
  - mark it `processing`
  - sleep briefly
  - write a fake HLS manifest key
  - mark it `ready`
  - ack the Redis message
- Add structured logs for each transition.
- Verify a newly uploaded video reaches `ready` without real transcoding.

### Status API
- Add `GET /api/videos/:id/status`.
- Define JSON response shape.
- Include:
  - `id`
  - `status`
  - raw stream URL
  - HLS URL if available
  - `error_msg` if failed
- Add route tests.

### SSE Events
- Add `GET /api/videos/:id/events`.
- Emit a first status event immediately on connect.
- Emit keepalive events or comments.
- Add a basic SSE test or smoke check.

### Media Validation
- Decide `ffprobe` command line and output parsing approach.
- Add worker helper to run `ffprobe`.
- Classify validation failure as permanent.
- Persist `error_msg`.
- Ack failed jobs.

### HLS Generation
- Decide working directory layout for temp transcode files.
- Add worker helper to run deterministic `ffmpeg`.
- Upload manifest and segments to MinIO.
- Persist real `hls_manifest_key`.
- Mark `ready`.
- Ack the message only after success.

### Retry / Reclaim / DLQ
- Define transient vs permanent error classification.
- Implement retry counter logic.
- Implement reclaim for stale pending entries.
- Add DLQ write on retry exhaustion.
- Add focused tests around retry / reclaim behavior.
