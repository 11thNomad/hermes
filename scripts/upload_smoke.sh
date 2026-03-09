#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

API_BASE_URL="${API_BASE_URL:-http://localhost:8080}"
VIDEO_PATH="${VIDEO_PATH:-$ROOT_DIR/test-videos/drums.webm}"

if [[ ! -f "$VIDEO_PATH" ]]; then
  echo "video file not found: $VIDEO_PATH" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required for JSON parsing" >&2
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required" >&2
  exit 1
fi

wait_for_url() {
  local url="$1"
  local attempts="${2:-30}"

  for ((i = 1; i <= attempts; i++)); do
    if curl -fsS "$url" >/dev/null; then
      return 0
    fi
    sleep 1
  done

  echo "timed out waiting for $url" >&2
  return 1
}

wait_for_url "$API_BASE_URL/healthz"

response_file="$(mktemp)"
headers_file="$(mktemp)"
trap 'rm -f "$response_file" "$headers_file"' EXIT

status_code="$(
  curl -sS \
  -D "$headers_file" \
  -o "$response_file" \
  -w '%{http_code}' \
  -F "video=@${VIDEO_PATH};type=video/webm" \
  "$API_BASE_URL/api/videos"
)"
if [[ "$status_code" != "201" ]]; then
  echo "upload failed with status $status_code" >&2
  cat "$response_file" >&2
  exit 1
fi

video_id="$(python3 - "$response_file" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    payload = json.load(fh)

print(payload["id"])
PY
)"

shareable_url="$(python3 - "$response_file" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    payload = json.load(fh)

print(payload["shareable_url"])
PY
)"

stream_headers="$(mktemp)"
range_headers="$(mktemp)"
trap 'rm -f "$response_file" "$headers_file" "$stream_headers" "$range_headers"' EXIT
stream_status="$(
  curl -sS \
    -D "$stream_headers" \
    -o /dev/null \
    -w '%{http_code}' \
    "$API_BASE_URL/api/videos/$video_id/stream"
)"

if [[ "$stream_status" != "200" ]]; then
  echo "stream check failed with status $stream_status for video $video_id" >&2
  exit 1
fi

range_status="$(
  curl -sS \
    -D "$range_headers" \
    -o /dev/null \
    -w '%{http_code}' \
    -H 'Range: bytes=0-1023' \
    "$API_BASE_URL/api/videos/$video_id/stream"
)"

if [[ "$range_status" != "206" ]]; then
  echo "range check failed with status $range_status for video $video_id" >&2
  exit 1
fi

if ! grep -qi '^accept-ranges: bytes' "$range_headers"; then
  echo "range check failed: missing Accept-Ranges header" >&2
  exit 1
fi

if ! grep -qi '^content-range: bytes 0-1023/' "$range_headers"; then
  echo "range check failed: missing or invalid Content-Range header" >&2
  exit 1
fi

echo "Upload smoke check passed."
echo "video_id=$video_id"
echo "shareable_url=$shareable_url"
echo "stream_url=$API_BASE_URL/api/videos/$video_id/stream"
