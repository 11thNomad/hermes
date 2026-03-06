# TODO

## Deferred Low-Priority Review Items

- [ ] Split shared config as API-only and worker-only fields grow.
- [ ] Improve dev-container caching for Rust builds instead of relying on full workspace mounts with `cargo run`.
- [ ] Remove or rethink the fallback title in `frontend/src/routes/+layout.svelte`.
- [ ] Harden `frontend/src/lib/api.ts` so `apiUrl()` enforces or documents leading-slash paths.
