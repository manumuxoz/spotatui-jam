#!/usr/bin/env bash
# Clone spotatui v0.48.0, apply the Party Queue patch and build the YouTube
# party binary. The clone is left in vendor/spotatui and can be reused.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${1:-${ROOT}/vendor/spotatui}"
PATCH="${ROOT}/patches/0001-party-queue.patch"

if [[ ! -d "${DEST}/.git" ]]; then
  mkdir -p "$(dirname "${DEST}")"
  git clone --depth 1 --branch v0.48.0 \
    https://github.com/LargeModGames/spotatui.git "${DEST}"
fi

cd "${DEST}"

if git apply --reverse --check "${PATCH}" >/dev/null 2>&1; then
  echo "patch already applied in ${DEST}"
elif git apply --check "${PATCH}" >/dev/null 2>&1; then
  git apply "${PATCH}"
  echo "patch applied in ${DEST}"
else
  echo "patch does not apply cleanly; expected a clean v0.48.0 tree" >&2
  exit 1
fi

if ! command -v yt-dlp >/dev/null 2>&1; then
  echo "warning: yt-dlp is not on PATH; YouTube playback will not work" >&2
fi

cargo build --locked --no-default-features \
  --features telemetry,tui,streaming,youtube

cat <<EOF
built: ${DEST}/target/debug/spotatui

Start the relay in another terminal:
  cd ${ROOT}/party-relay && cargo run --release

Then run:
  ${DEST}/target/debug/spotatui

Set behavior.relay_server_url to ws://127.0.0.1:8080/ws.
EOF
