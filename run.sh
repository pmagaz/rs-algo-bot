#!/usr/bin/env bash
set -euo pipefail

# Load .env
if [ -f .env ]; then
  export $(grep -v '^\s*#' .env | grep -v '^\s*$' | xargs)
fi

BROKER="${BROKER:-oanda}"
CARGO="${HOME}/.cargo/bin/cargo"

die() {
  echo "[run] ERROR: $*" >&2
  exit 1
}

cleanup() {
  echo ""
  echo "[run] Shutting down..."
  kill "$SERVER_PID" "$BOT_PID" 2>/dev/null || true
  if [ "$BROKER" = "ibkr" ]; then
    docker-compose stop ibeam 2>/dev/null || true
  fi
  exit 0
}
trap cleanup INT TERM

# ── IBKR: validate credentials and start ibeam ────────────────────────────────
if [ "$BROKER" = "ibkr" ]; then
  [ -z "${BROKER_USERNAME:-}" ] && die "BROKER_USERNAME is not set in .env"
  [ -z "${BROKER_PASSWORD:-}" ] && die "BROKER_PASSWORD is not set in .env"

  echo "[run] BROKER=ibkr — starting ibeam sidecar..."
  docker-compose up -d ibeam

  echo "[run] Waiting for ibeam to authenticate (up to 120s)..."
  for i in $(seq 1 12); do
    sleep 10

    # Fail fast: wrong credentials detected in logs
    if docker-compose logs ibeam 2>&1 | grep -q "Invalid username password combination"; then
      die "IBKR login failed — invalid credentials for user '${BROKER_USERNAME}'. Check BROKER_USERNAME and BROKER_PASSWORD in .env"
    fi

    if curl -k -sf https://localhost:5000/v1/api/iserver/auth/status \
         2>/dev/null | grep -q '"authenticated":true'; then
      echo "[run] ibeam authenticated."
      break
    fi

    echo "[run] Not authenticated yet (attempt $i/12)..."

    if [ "$i" = "12" ]; then
      echo "[run] ibeam logs:"
      docker-compose logs --tail=20 ibeam 2>&1
      die "ibeam failed to authenticate after 120s"
    fi
  done
fi

# ── Build ──────────────────────────────────────────────────────────────────────
echo "[run] Building workspace..."
"$CARGO" build 2>&1

# ── Start server ───────────────────────────────────────────────────────────────
echo "[run] Starting ws server..."
"$CARGO" run -p rs_algo_ws_server &
SERVER_PID=$!

sleep 2

# ── Start bot ──────────────────────────────────────────────────────────────────
echo "[run] Starting bot..."
"$CARGO" run -p rs_algo_bot &
BOT_PID=$!

echo "[run] Running. Press Ctrl+C to stop."
wait "$SERVER_PID" "$BOT_PID"
