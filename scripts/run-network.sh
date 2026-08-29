#!/usr/bin/env bash
# Start JoveWorks Hub for access beyond this WSL instance.
#
# Configuration belongs in .env (copied from .env.example), never in this
# script or the shell history. `.env` is intentionally sourced as shell-style
# KEY=value configuration; keep it local and do not use one from an untrusted
# source.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [[ -f .env ]]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

: "${JOVEWORKS_ADMIN_TOKEN:?Copy .env.example to .env and set JOVEWORKS_ADMIN_TOKEN.}"
if [[ -z "${JOVEWORKS_CLOUD_TOKEN:-}" ]]; then
  echo "JOVEWORKS_CLOUD_TOKEN is unset: restricted catalogues will remain unavailable." >&2
fi

# Unlike `cargo run`'s development default, this listens on every IPv4
# interface so WSL mirrored networking or a reverse proxy can reach Hub.
export JOVEWORKS_BIND="${JOVEWORKS_BIND:-0.0.0.0:8080}"

exec cargo run --release
