# Hermes

Hermes is a phased implementation of a private video upload and streaming service using Rust, SvelteKit, PostgreSQL, Redis Streams, and MinIO.

## Phase Status

- Phase 1 is scaffolded: Rust workspace, shared bootstrap flow, container layout, migrations directory, frontend shell, and DX tooling.
- Phases 2 through 5 are still pending.

## Quick Start

1. Copy `.env.example` to `.env`.
2. Run `make bootstrap`.
3. Run `make dev`.

## Notes

- Both the API and worker use the shared bootstrap code in `crates/common` to run migrations, create MinIO buckets, and ensure the Redis consumer group exists.
- The current API only exposes placeholder health routes; upload and streaming routes arrive in Phase 2.
