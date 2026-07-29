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

cleanup() {
  echo
  echo "Stopping platform processes..."
  kill "$BACKEND_PID" "$DESKTOP_PID" 2>/dev/null || true
  wait "$BACKEND_PID" 2>/dev/null || true
  wait "$DESKTOP_PID" 2>/dev/null || true
}

trap cleanup INT TERM EXIT

wait -n "$BACKEND_PID" "$DESKTOP_PID"
EXIT_CODE=$?

if [ $EXIT_CODE -ne 0 ]; then
  echo "A process exited with code $EXIT_CODE" >&2
fi

exit $EXIT_CODE
