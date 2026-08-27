#!/usr/bin/env bash
# Upload one immutable catalogue revision and publish a NodeBook into a course.
#
# Usage: scripts/publish-nodebook.sh <course-slug> <catalogue.json> <version> <nodebook.jove.json>
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [[ -f .env ]]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

if [[ $# -ne 4 ]]; then
  echo "usage: $0 <course-slug> <catalogue.json> <version> <nodebook.jove.json>" >&2
  exit 2
fi

: "${JOVEWORKS_ADMIN_TOKEN:?Set JOVEWORKS_ADMIN_TOKEN in .env or the environment.}"
if ! command -v python3 >/dev/null 2>&1; then
  echo "publish-nodebook.sh requires python3 for safe JSON and URL encoding" >&2
  exit 1
fi

course_slug="$1"
catalogue_file="$2"
catalogue_version="$3"
nodebook_file="$4"
hub_url="${JOVEWORKS_HUB_URL:-http://localhost:8080}"
hub_url="${hub_url%/}"

if [[ ! -r "$catalogue_file" || ! -r "$nodebook_file" ]]; then
  echo "catalogue and NodeBook paths must name readable JSON files" >&2
  exit 2
fi
if ! [[ "$catalogue_version" =~ ^[1-9][0-9]*$ ]]; then
  echo "catalogue version must be a positive integer" >&2
  exit 2
fi

catalogue_id="$(python3 -c '
import json, sys
with open(sys.argv[1], encoding="utf-8") as source: value = json.load(source)
identifier = value.get("id") if isinstance(value, dict) else None
if not isinstance(identifier, str) or not identifier.strip(): raise SystemExit("catalogue JSON must contain a non-empty string id")
print(identifier)
' "$catalogue_file")"
encoded_course="$(python3 -c 'from urllib.parse import quote; import sys; print(quote(sys.argv[1], safe=""))' "$course_slug")"
encoded_catalogue="$(python3 -c 'from urllib.parse import quote; import sys; print(quote(sys.argv[1], safe=""))' "$catalogue_id")"

catalogue_payload="$(python3 -c '
import json, sys
with open(sys.argv[1], encoding="utf-8") as source: content = json.load(source)
print(json.dumps({"content": content}, ensure_ascii=False, separators=(",", ":")))
' "$catalogue_file")"
catalogue_response="$(curl --silent --show-error --fail-with-body \
  --request POST \
  --header 'Content-Type: application/json' \
  --header "X-JoveWorks-Admin-Token: ${JOVEWORKS_ADMIN_TOKEN}" \
  --data-binary "$catalogue_payload" \
  "${hub_url}/api/v1/catalogues/${encoded_catalogue}/${catalogue_version}")"

publication_payload="$(python3 -c '
import json, sys
catalogue_response, course, version, nodebook_path = sys.argv[1:]
catalogue = json.loads(catalogue_response)
with open(nodebook_path, encoding="utf-8") as source: document = json.load(source)
title = document.get("title") if isinstance(document, dict) else None
if not isinstance(title, str) or not title.strip(): raise SystemExit("NodeBook JSON must contain a non-empty string title")
reference = {key: catalogue.get(key) for key in ("id", "version", "hash")}
if not isinstance(reference["id"], str) or not isinstance(reference["version"], int) or not isinstance(reference["hash"], str): raise SystemExit("Hub returned an invalid catalogue upload response")
payload = {"title": title, "mode": "editor", "document": document, "catalogues": [reference], "courses": [course]}
print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
' "$catalogue_response" "$course_slug" "$catalogue_version" "$nodebook_file")"
publication_response="$(curl --silent --show-error --fail-with-body \
  --request POST \
  --header 'Content-Type: application/json' \
  --header "X-JoveWorks-Admin-Token: ${JOVEWORKS_ADMIN_TOKEN}" \
  --data-binary "$publication_payload" \
  "${hub_url}/api/v1/publications")"
publication_id="$(python3 -c '
import json, sys
identifier = json.loads(sys.argv[1]).get("id")
if not isinstance(identifier, str): raise SystemExit("Hub returned an invalid publication response")
print(identifier)
' "$publication_response")"

echo "Published: ${hub_url}/p/${publication_id}"
