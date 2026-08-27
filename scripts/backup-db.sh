#!/usr/bin/env bash
# Create a consistent SQLite backup. Requires sqlite3 and a local database URL.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
[[ -f .env ]] && { set -a; source .env; set +a; }
: "${JOVEWORKS_DATABASE_URL:=sqlite://joveworks-hub.sqlite?mode=rwc}"
command -v sqlite3 >/dev/null 2>&1 || { echo "backup-db.sh requires sqlite3" >&2; exit 1; }
database="${JOVEWORKS_DATABASE_URL#sqlite://}"
database="${database%%\?*}"
mkdir -p backups
destination="backups/joveworks-hub-$(date -u +%Y%m%dT%H%M%SZ).sqlite"
sqlite3 "$database" ".backup '$destination'"
echo "$destination"
