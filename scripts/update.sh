#!/usr/bin/env bash
# Update Hub from the current branch and recreate the production service.
# The joveworks-data named volume (and therefore its SQLite database) is kept.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

git pull --ff-only
docker compose -f compose.production.yaml build --pull hub
docker compose -f compose.production.yaml up -d
docker compose -f compose.production.yaml ps
