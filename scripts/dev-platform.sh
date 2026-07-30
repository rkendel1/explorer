#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="$ROOT_DIR/apps/desktop"

BACKEND_PORT="${BACKEND_PORT:-4010}"
REPOSITORY_PATH="${REPOSITORY_PATH:-$ROOT_DIR}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is not installed or not in PATH" >&2
  exit 1
fi

if ! command -v npm >/dev/null 2>&1; then
  echo "error: npm is not installed or not in PATH" >&2
  exit 1
fi

if [ ! -d "$DESKTOP_DIR/node_modules" ]; then
  echo "Installing desktop dependencies..."
  (cd "$DESKTOP_DIR" && npm install)
fi

echo "Starting mock backend on port $BACKEND_PORT..."
(
  cd "$ROOT_DIR"
  cargo run -p api-cli -- mock "$REPOSITORY_PATH" --port "$BACKEND_PORT" --stateful
) &
BACKEND_PID=$!

echo "Starting desktop app (Tauri dev)..."
(
  cd "$DESKTOP_DIR"
  npm run tauri -- dev
) &
DESKTOP_PID=$!

# cargo/npm each spawn their own child processes (the compiled binary,
# vite, the tauri app). Killing just $BACKEND_PID/$DESKTOP_PID leaves those
# grandchildren running as orphans, so walk the tree instead.
kill_tree() {
  local pid="$1"
  local child
  for child in $(pgrep -P "$pid" 2>/dev/null); do
    kill_tree "$child"
  done
  kill "$pid" 2>/dev/null || true
}

cleanup() {
  echo
  echo "Stopping platform processes..."
  kill_tree "$BACKEND_PID"
  kill_tree "$DESKTOP_PID"
  wait "$BACKEND_PID" 2>/dev/null || true
  wait "$DESKTOP_PID" 2>/dev/null || true
}

trap cleanup INT TERM EXIT

# `wait -n` needs bash >= 4.3, but macOS ships bash 3.2, so poll instead.
EXIT_CODE=0
while true; do
  if ! kill -0 "$BACKEND_PID" 2>/dev/null; then
    wait "$BACKEND_PID"
    EXIT_CODE=$?
    break
  fi
  if ! kill -0 "$DESKTOP_PID" 2>/dev/null; then
    wait "$DESKTOP_PID"
    EXIT_CODE=$?
    break
  fi
  sleep 1
done

if [ $EXIT_CODE -ne 0 ]; then
  echo "A process exited with code $EXIT_CODE" >&2
fi

exit $EXIT_CODE
