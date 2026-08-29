#!/usr/bin/env bash
# Conformance check for any JoveWorks Hub API v1 implementation — including
# one that isn't this codebase at all. Point it at a Hub deployment (this
# reference server or your own from-scratch implementation) and it exercises
# the contract documented in docs/API-v1.md end to end: discovery, clouds,
# canonical catalogue hashing, publications, and student workspaces.
#
# It creates and cleans up its own randomly-suffixed cloud/catalogue/
# workspace/publication so it is safe to run against a real deployment; it
# never modifies pre-existing data. It fails fast and prints which check
# failed. Requires: curl, python3.
#
# Usage:
#   ./scripts/conformance-check.sh <hub-url> <admin-token> [cloud-token]
#
# Example:
#   ./scripts/conformance-check.sh http://127.0.0.1:8080 my-admin-token

set -euo pipefail

HUB="${1:?usage: conformance-check.sh <hub-url> <admin-token> [cloud-token]}"
ADMIN_TOKEN="${2:?usage: conformance-check.sh <hub-url> <admin-token> [cloud-token]}"
CLOUD_TOKEN="${3:-}"
HUB="${HUB%/}"

RUN_ID="conformance-$(date +%s)-$$"
SLUG="${RUN_ID}"
CATALOGUE_ID="${RUN_ID}"
FAILURES=0
PASSES=0

pass() { PASSES=$((PASSES + 1)); echo "  ok  - $1"; }
fail() { FAILURES=$((FAILURES + 1)); echo "FAIL  - $1"; }

# request METHOD PATH [BODY] [EXTRA_CURL_ARGS...]
# Prints "<status>\n<body>" — status on its own first line, body after.
request() {
  local method="$1" path="$2" body="${3:-}"
  shift 3 2>/dev/null || shift $#
  if [ -n "$body" ]; then
    curl -s -o /tmp/conformance-body.$$ -w '%{http_code}' -X "$method" "$HUB$path" \
      -H 'Content-Type: application/json' -d "$body" "$@"
  else
    curl -s -o /tmp/conformance-body.$$ -w '%{http_code}' -X "$method" "$HUB$path" "$@"
  fi
  echo
  cat /tmp/conformance-body.$$
  rm -f /tmp/conformance-body.$$
}

json_get() { # json_get <field> <<< body
  python3 -c "import sys,json; v=json.load(sys.stdin); print(v$1)"
}

check_status() {
  local label="$1" expected="$2" actual="$3"
  if [ "$actual" = "$expected" ]; then pass "$label ($actual)"; else fail "$label (expected $expected, got $actual)"; fi
}

echo "== Discovery and health =="
resp=$(request GET /.well-known/joveworks); status=$(echo "$resp" | head -1); body=$(echo "$resp" | tail -n +2)
check_status "GET /.well-known/joveworks" 200 "$status"
[ "$(echo "$body" | json_get "['protocolVersion']")" = "1" ] && pass "protocolVersion is 1" || fail "protocolVersion is 1"

status=$(request GET /healthz | head -1)
check_status "GET /healthz" 204 "$status"

echo "== Clouds =="
status=$(request POST "/api/v1/clouds/$SLUG" '{"title":"Conformance cloud"}' | head -1)
check_status "POST cloud without admin token -> 401" 401 "$status"

status=$(request POST "/api/v1/clouds/$SLUG" '{"title":"Conformance cloud"}' -H "X-JoveWorks-Admin-Token: $ADMIN_TOKEN" | head -1)
check_status "POST cloud with admin token -> 204" 204 "$status"

resp=$(request GET "/api/v1/clouds/$SLUG"); status=$(echo "$resp" | head -1); body=$(echo "$resp" | tail -n +2)
check_status "GET cloud" 200 "$status"
[ "$(echo "$body" | json_get "['slug']")" = "$SLUG" ] && pass "cloud slug round-trips" || fail "cloud slug round-trips"

echo "== Canonical catalogue hashing =="
# Keys deliberately out of alphabetical order: the server must canonicalize.
CONTENT='{"restricted":false,"schemaVersion":1,"id":"'"$CATALOGUE_ID"'","formulas":[]}'
EXPECTED_HASH=$(python3 -c "
import json, hashlib
content = json.loads('''$CONTENT''')
canonical = json.dumps(content, sort_keys=True, separators=(',', ':'))
print(hashlib.sha256(canonical.encode()).hexdigest())
")

resp=$(request POST "/api/v1/catalogues/$CATALOGUE_ID/1" "{\"content\":$CONTENT}" -H "X-JoveWorks-Admin-Token: $ADMIN_TOKEN")
status=$(echo "$resp" | head -1); body=$(echo "$resp" | tail -n +2)
check_status "POST catalogue" 200 "$status"
ACTUAL_HASH=$(echo "$body" | json_get "['hash']" 2>/dev/null || echo "")
if [ "$ACTUAL_HASH" = "$EXPECTED_HASH" ]; then
  pass "hash matches independently-computed canonical SHA-256"
else
  fail "hash mismatch: server returned $ACTUAL_HASH, expected $EXPECTED_HASH (canonicalization does not match spec)"
fi

status=$(request POST "/api/v1/catalogues/$CATALOGUE_ID/1" "{\"content\":$CONTENT}" -H "X-JoveWorks-Admin-Token: $ADMIN_TOKEN" | head -1)
check_status "re-upload same (id, version) -> 409" 409 "$status"

status=$(request GET "/api/v1/catalogues/$CATALOGUE_ID/1" | head -1)
check_status "GET public catalogue -> 200" 200 "$status"

echo "== Cloud catalogue pins =="
PIN="[{\"id\":\"$CATALOGUE_ID\",\"version\":1,\"hash\":\"$ACTUAL_HASH\"}]"
status=$(request PUT "/api/v1/clouds/$SLUG/catalogues" "{\"catalogues\":$PIN}" -H "X-JoveWorks-Admin-Token: $ADMIN_TOKEN" | head -1)
check_status "PUT cloud catalogues" 204 "$status"

echo "== Publications =="
DOC="{\"schemaVersion\":1,\"id\":\"$RUN_ID-doc\"}"
resp=$(request POST /api/v1/publications "{\"title\":\"Conformance publication\",\"document\":$DOC,\"catalogues\":$PIN,\"clouds\":[\"$SLUG\"]}" -H "X-JoveWorks-Admin-Token: $ADMIN_TOKEN")
status=$(echo "$resp" | head -1); body=$(echo "$resp" | tail -n +2)
check_status "POST publication" 201 "$status"
PUB_ID=$(echo "$body" | json_get "['id']" 2>/dev/null || echo "")
if [ -n "$PUB_ID" ]; then
  status=$(request GET "/api/v1/publications/$PUB_ID" | head -1)
  check_status "GET publication" 200 "$status"
  status=$(request GET "/p/$PUB_ID" | head -1)
  check_status "GET /p/{id} -> 307" 307 "$status"
else
  fail "publication id present in response"
fi

echo "== Deletion is blocked while referenced =="
status=$(request DELETE "/api/v1/admin/catalogues/$CATALOGUE_ID/1" "" -H "X-JoveWorks-Admin-Token: $ADMIN_TOKEN" | head -1)
check_status "DELETE in-use catalogue -> 409" 409 "$status"

echo "== Student workspaces =="
resp=$(request POST /api/v1/workspaces "{\"title\":\"Conformance workspace\",\"document\":{\"schemaVersion\":1,\"id\":\"$RUN_ID-ws\"}}")
status=$(echo "$resp" | head -1); body=$(echo "$resp" | tail -n +2)
check_status "POST workspace (no admin token needed)" 201 "$status"
WS_ID=$(echo "$body" | json_get "['id']" 2>/dev/null || echo "")
WS_TOKEN=$(echo "$body" | json_get "['editToken']" 2>/dev/null || echo "")

if [ -n "$WS_ID" ] && [ -n "$WS_TOKEN" ]; then
  status=$(request GET "/api/v1/workspaces/$WS_ID" | head -1)
  check_status "GET workspace without token -> 401" 401 "$status"

  status=$(request GET "/api/v1/workspaces/$WS_ID" "" -H "X-JoveWorks-Workspace-Token: $WS_TOKEN" | head -1)
  check_status "GET workspace with token -> 200" 200 "$status"

  status=$(request PUT "/api/v1/workspaces/$WS_ID" "{\"title\":\"Renamed\",\"document\":{\"schemaVersion\":1,\"id\":\"$RUN_ID-ws\"}}" -H "X-JoveWorks-Workspace-Token: wrong-token" | head -1)
  check_status "PUT workspace with wrong token -> 401" 401 "$status"

  status=$(request DELETE "/api/v1/workspaces/$WS_ID" "" -H "X-JoveWorks-Workspace-Token: $WS_TOKEN" | head -1)
  check_status "DELETE workspace with token -> 204" 204 "$status"
else
  fail "workspace id and editToken present in response"
fi

if [ -n "$CLOUD_TOKEN" ]; then
  echo "== Restricted catalogues (cloud token supplied) =="
  RESTRICTED_ID="${RUN_ID}-restricted"
  RESTRICTED_CONTENT="{\"schemaVersion\":1,\"id\":\"$RESTRICTED_ID\",\"restricted\":true,\"formulas\":[]}"
  status=$(request POST "/api/v1/catalogues/$RESTRICTED_ID/1" "{\"content\":$RESTRICTED_CONTENT}" -H "X-JoveWorks-Admin-Token: $ADMIN_TOKEN" | head -1)
  check_status "POST restricted catalogue" 200 "$status"

  status=$(request GET "/api/v1/catalogues/$RESTRICTED_ID/1" | head -1)
  check_status "GET restricted catalogue without cloud token -> 401" 401 "$status"

  status=$(request GET "/api/v1/catalogues/$RESTRICTED_ID/1" "" -H "X-JoveWorks-Cloud-Token: $CLOUD_TOKEN" | head -1)
  check_status "GET restricted catalogue with cloud token -> 200" 200 "$status"
else
  echo "== Restricted catalogues skipped (no cloud token argument given) =="
fi

echo
echo "== Summary: $PASSES passed, $FAILURES failed =="
[ "$FAILURES" -eq 0 ]
