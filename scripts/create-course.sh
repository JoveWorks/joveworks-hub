#!/usr/bin/env bash
# Create or update one course in a local JoveWorks Hub.
#
# Usage: scripts/create-course.sh <slug> <title>
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [[ -f .env ]]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <slug> <title>" >&2
  exit 2
fi

: "${JOVEWORKS_ADMIN_TOKEN:?Set JOVEWORKS_ADMIN_TOKEN in .env or the environment.}"
hub_url="${JOVEWORKS_HUB_URL:-http://localhost:8080}"
hub_url="${hub_url%/}"
slug="$1"
title="$2"

# jq is preferred when present; Python's standard library is the fallback so
# titles containing quotes, newlines, or non-ASCII characters remain valid JSON.
if command -v jq >/dev/null 2>&1; then
  payload="$(jq -cn --arg title "$title" '{title: $title}')"
  encoded_slug="$(printf '%s' "$slug" | jq -sRr @uri)"
elif command -v python3 >/dev/null 2>&1; then
  payload="$(python3 -c 'import json,sys; print(json.dumps({"title":sys.argv[1]}, ensure_ascii=False, separators=(",", ":")))' "$title")"
  encoded_slug="$(python3 -c 'from urllib.parse import quote; import sys; print(quote(sys.argv[1], safe=""))' "$slug")"
else
  echo "create-course.sh requires jq or python3 for safe encoding" >&2
  exit 1
fi

curl --silent --show-error --fail-with-body \
  --request POST \
  --header 'Content-Type: application/json' \
  --header "X-JoveWorks-Admin-Token: ${JOVEWORKS_ADMIN_TOKEN}" \
  --data-binary "$payload" \
  "${hub_url}/api/v1/courses/${encoded_slug}"
