#!/bin/sh
set -eu

compose="${COMPOSE_BIN:-docker compose}"
app_port="${APP_PORT:-8000}"
bank_port="${BANK_PORT:-5001}"

wait_http() {
  url=$1
  attempts=0
  while [ "$attempts" -lt 120 ]; do
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    attempts=$((attempts + 1))
    sleep 1
  done
  echo "timeout waiting for $url" >&2
  return 1
}

eval "$compose up --build -d"
trap 'eval "$compose down -v"' EXIT

wait_http "${BASE_URL:-http://localhost:${app_port}}/health"
wait_http "${BANK_URL:-http://localhost:${bank_port}}/stats"

python3 -m pip install -q -r tests/requirements.txt
BASE_URL="${BASE_URL:-http://localhost:${app_port}}" \
  BANK_URL="${BANK_URL:-http://localhost:${bank_port}}" \
  python3 -m pytest -q tests/test_blackbox_smoke.py

# Exercise the maintenance command surface against the running container.
$compose exec -T app shopcore init-db
$compose exec -T app shopcore seed
$compose exec -T app shopcore purge-sessions
$compose exec -T app sh -c \
  'printf "sku,name,price,stock\nCLI-SMOKE-1,CLI Smoke,1.5,2\n" > /tmp/cli-import.csv && shopcore import-products /tmp/cli-import.csv'
$compose exec -T app shopcore export-sales /tmp/exports/cli-sales.csv
$compose exec -T app test -s /tmp/exports/cli-sales.csv
