# FAST

This file describes the absolute shortest time-to-stream architecture for Hermes.

## Why It Is Different

The implemented direct-to-MinIO flow is the fastest path to ship because it removes API temp-file buffering and keeps the backend out of the upload hot path.

It is not the lowest possible time-to-stream because an object uploaded with a single presigned `PUT` only becomes readable after that request completes.

If the primary metric is the earliest possible moment another tab can begin playback, the ingest path has to stay inside the API.

## Absolute-Fastest Architecture

1. `POST /api/uploads/init` mints a `video_id` and returns the shareable watch URL immediately.
2. The browser uploads to an API ingest endpoint such as `PUT /api/uploads/:id/body`.
3. The API writes incoming bytes to a growing temp file on local disk.
4. The raw stream endpoint reads from that same growing file while the upload is still in progress.
5. The watch page can begin raw playback as soon as enough container metadata and media bytes are available.
6. After upload completion, the API persists the raw source to object storage and enqueues the transcode job.
7. HLS generation remains asynchronous and upgrades playback later.

## Practical Notes

- Do not store full video bodies in Redis. For 1 GB uploads, Redis is the wrong medium for cost, durability, and range-serving behavior.
- A small in-memory buffer can still help smooth the head of the stream, but disk-backed ingest should be the source of truth.
- Early raw playback depends on the container format. Fast-start MP4 and some WebM files behave much better than files whose critical metadata sits at the end.
- This design improves time-to-stream at the cost of a more complex API lifecycle:
  - active upload registry
  - growing-file range serving
  - cleanup for interrupted uploads
  - object-storage persistence after or during ingest

## Recommendation

For this exercise, direct-to-MinIO is the best speed-to-complexity tradeoff.

If Hermes were pushed further toward pure time-to-stream optimization, the next step would be:

1. keep the new shareable-link-first flow
2. replace browser-to-MinIO upload with API ingest
3. allow `/stream` to serve from an active growing file before upload completion
