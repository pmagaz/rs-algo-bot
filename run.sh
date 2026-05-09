#!/usr/bin/env bash
set -euo pipefail

if [ -f .env ]; then
  export $(grep -v '^\s*#' .env | grep -v '^\s*$' | xargs)
fi

BROKER="${BROKER:-oanda}"
CARGO="${HOME}/.cargo/bin/cargo"
IBEAM_LOG_PID=""

cleanup() {
  echo ""
  kill "$SERVER_PID" "$BOT_PID" 2>/dev/null || true
  [ -n "$IBEAM_LOG_PID" ] && kill "$IBEAM_LOG_PID" 2>/dev/null || true
  # Keep ibeam running to preserve the authenticated session across restarts
  exit 0
}
trap cleanup INT TERM

if [ "$BROKER" = "ibkr" ]; then
  docker-compose up -d ibeam
  docker-compose exec -d ibeam python3 /srv/inputs/proxy.py 2>/dev/null || true
  IBKR_PROXY_PORT="${IBKR_PROXY_PORT:-5100}"
  export IBKR_GATEWAY_URL="https://localhost:${IBKR_PROXY_PORT}"
  docker-compose logs -f --no-log-prefix ibeam 2>&1 | sed 's/^/[ibeam] /' &
  IBEAM_LOG_PID=$!

  echo "Waiting for ibeam to authenticate..."
  until curl -sf http://localhost:5001/readyz > /dev/null 2>&1; do
    sleep 5
  done
  echo "ibeam ready — starting server and bot"
fi

"$CARGO" build 2>&1

"$CARGO" run -p rs_algo_ws_server &
SERVER_PID=$!

sleep 2

"$CARGO" run -p rs_algo_bot &
BOT_PID=$!

wait "$SERVER_PID" "$BOT_PID"
