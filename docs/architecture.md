# Hermes Architecture

This document will track the implementation as the system moves through the planned phases.

## Current Status

Phase 1 provides the monorepo scaffold, shared bootstrap flow, container layout, and a minimal frontend shell. The upload, streaming, transcoding, and watch flows are still pending.

## Intended Architecture

- `crates/api` will own the HTTP API, upload handling, raw streaming, status endpoints, and HLS delivery.
- `crates/worker` will consume Redis Stream jobs and run ffprobe/ffmpeg-based HLS generation.
- `crates/common` centralizes config loading, object storage setup, Redis bootstrap, and shared domain models.
- `frontend` is a SvelteKit app that will cover upload and watch flows.

