#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

wait_for_url() {
  local url="$1"
  local attempts="${2:-30}"

  for ((i = 1; i <= attempts; i++)); do
    if curl -fs "$url" >/dev/null; then
      return 0
    fi
    sleep 1
  done

  echo "timed out waiting for $url" >&2
  return 1
}

docker compose up -d

wait_for_url "http://localhost:8080/healthz"
wait_for_url "http://localhost:8080/api/healthz"
wait_for_url "http://localhost:4173"

health_payload="$(curl -fsS http://localhost:8080/healthz)"
api_health_payload="$(curl -fsS http://localhost:8080/api/healthz)"
root_payload="$(curl -fsS http://localhost:8080/)"

grep -q '"kind":"health"' <<<"$health_payload"
grep -q '"kind":"health"' <<<"$api_health_payload"
grep -q '"kind":"info"' <<<"$root_payload"

docker exec hermes-minio-1 mc alias set local http://127.0.0.1:9000 minioadmin minioadmin >/dev/null
bucket_listing="$(docker exec hermes-minio-1 mc ls local)"
grep -q 'videos-hls/' <<<"$bucket_listing"
grep -q 'videos-raw/' <<<"$bucket_listing"

group_info="$(docker exec hermes-redis-1 redis-cli XINFO GROUPS transcode_jobs)"
grep -q '^workers$' <<<"$group_info"

echo "Phase 1 smoke check passed."
