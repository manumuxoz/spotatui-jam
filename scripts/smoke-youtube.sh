#!/usr/bin/env bash
# Smoke test for the YouTube source and the URIs used by Party Queue.
# Requires yt-dlp and network access. Does not download audio.
set -euo pipefail

VIDEO_ID="${1:-dQw4w9WgXcQ}"
URL="https://www.youtube.com/watch?v=${VIDEO_ID}"

if ! command -v yt-dlp >/dev/null 2>&1; then
  echo "yt-dlp is required" >&2
  exit 2
fi

line="$(yt-dlp --skip-download --no-warnings \
  --print "%(id)s|%(title)s|%(duration)s" "$URL")"
id="${line%%|*}"
rest="${line#*|}"
title="${rest%%|*}"
duration="${rest##*|}"

if [[ "$id" != "$VIDEO_ID" ]]; then
  echo "unexpected video id: $id" >&2
  exit 1
fi
if [[ -z "$title" || "$duration" -le 0 ]]; then
  echo "missing metadata: $line" >&2
  exit 1
fi

echo "youtube OK: youtube:${id} | ${title} | ${duration}s"
