#!/usr/bin/env bash
set -euo pipefail

if [ -f .env ]; then
  export $(grep -v '^\s*#' .env | grep -v '^\s*$' | xargs)
fi

BROKER="${BROKER:-oanda}"
CARGO="${HOME}/.cargo/bin/cargo"

cleanup() {
  echo ""
  kill "$SERVER_PID" "$BOT_PID" 2>/dev/null || true
  [ "$BROKER" = "ibkr" ] && docker-compose stop ibeam 2>/dev/null || true
  exit 0
}
trap cleanup INT TERM

if [ "$BROKER" = "ibkr" ]; then
  docker-compose up -d ibeam
fi

"$CARGO" build 2>&1

"$CARGO" run -p rs_algo_ws_server &
SERVER_PID=$!

sleep 2

"$CARGO" run -p rs_algo_bot &
BOT_PID=$!

wait "$SERVER_PID" "$BOT_PID"
