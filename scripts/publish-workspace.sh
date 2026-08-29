#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "Usage: $0 WORKSPACE_ID TITLE MODE [CLOUD ...]" >&2
  exit 2
fi
: "${JOVEWORKS_ADMIN_TOKEN:?Set JOVEWORKS_ADMIN_TOKEN}"

workspace_id=$1
title=$2
mode=$3
shift 3
hub_url=${JOVEWORKS_HUB_URL:-http://127.0.0.1:8080}

payload=$(python3 -c '
import json, sys
workspace, title, mode, *clouds = sys.argv[1:]
if mode not in ("viewer", "editor"): raise SystemExit("MODE must be viewer or editor")
print(json.dumps({"workspaceId": workspace, "title": title, "mode": mode, "clouds": clouds}, separators=(",", ":")))
' "$workspace_id" "$title" "$mode" "$@")

curl --silent --show-error --fail-with-body \
  --request POST \
  --header 'Content-Type: application/json' \
  --header "X-JoveWorks-Admin-Token: ${JOVEWORKS_ADMIN_TOKEN}" \
  --data-binary "$payload" \
  "${hub_url%/}/api/v1/publications"
echo
