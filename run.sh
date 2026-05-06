#!/usr/bin/env bash
set -euo pipefail

# Load .env so we can read BROKER
if [ -f .env ]; then
  export $(grep -v '^\s*#' .env | grep -v '^\s*$' | xargs)
fi

BROKER="${BROKER:-oanda}"

cleanup() {
  echo ""
  echo "Shutting down..."
  kill "$SERVER_PID" "$BOT_PID" 2>/dev/null || true
  if [ "$BROKER" = "ibkr" ]; then
    docker-compose stop ibeam 2>/dev/null || true
  fi
  exit 0
}
trap cleanup INT TERM

# ── IBKR: start ibeam sidecar ─────────────────────────────────────────────────
if [ "$BROKER" = "ibkr" ]; then
  echo "[run] BROKER=ibkr — starting ibeam sidecar..."
  docker-compose up -d ibeam
  echo "[run] Waiting for ibeam to authenticate (up to 120s)..."
  for i in $(seq 1 12); do
    sleep 10
    if curl -k -sf https://localhost:5000/v1/api/iserver/auth/status \
         | grep -q '"authenticated":true'; then
      echo "[run] ibeam authenticated."
      break
    fi
    echo "[run] Not authenticated yet (attempt $i/12)..."
    if [ "$i" = "12" ]; then
      echo "[run] ERROR: ibeam failed to authenticate after 120s. Check logs with: docker-compose logs ibeam"
      exit 1
    fi
  done
fi

# ── Build ──────────────────────────────────────────────────────────────────────
echo "[run] Building workspace..."
~/.cargo/bin/cargo build 2>&1

# ── Start server ───────────────────────────────────────────────────────────────
echo "[run] Starting ws server..."
~/.cargo/bin/cargo run -p rs_algo_ws_server &
SERVER_PID=$!

# Give the server a moment to bind before the bot connects
sleep 2

# ── Start bot ──────────────────────────────────────────────────────────────────
echo "[run] Starting bot..."
~/.cargo/bin/cargo run -p rs_algo_bot &
BOT_PID=$!

echo "[run] Running. Press Ctrl+C to stop."
wait "$SERVER_PID" "$BOT_PID"
