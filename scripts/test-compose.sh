#!/bin/sh
set -eu

compose="${COMPOSE_BIN:-docker compose}"
eval "$compose up --build -d"
trap 'eval "$compose down -v"' EXIT

python3 -m pip install -q -r tests/requirements.txt
BASE_URL="${BASE_URL:-http://localhost:${APP_PORT:-8000}}" \
  BANK_URL="${BANK_URL:-http://localhost:${BANK_PORT:-5001}}" \
  python3 -m pytest -q tests/test_blackbox_smoke.py
