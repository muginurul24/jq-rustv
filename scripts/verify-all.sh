#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ -f .env ]]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

: "${DATABASE_URL:=postgresql://postgres:postgres@127.0.0.1:5432/justqiu}"
: "${REDIS_URL:=redis://127.0.0.1:6379/0}"

export DATABASE_URL
export REDIS_URL

cargo test --workspace -- --nocapture
npm --prefix apps/web run build
