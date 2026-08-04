#!/usr/bin/env bash

# L1 of plans/2026-08-04-ghcacher-plan.md 5.6. Talks to api.github.com over the
# operator's own gh credentials, so it is absent from green/green-all by design.
set -uo pipefail
TSV2="$(cd "$(dirname "$0")/.." && pwd)"
V6="$(cd "$TSV2/.." && pwd)"

# cap_self forks into its own process group and SIGKILLs it on overrun, which is
# what covers the background node server and its gh children. Measured wall ~5s.
. "$V6/tools/run-capped.sh"
cap_self "${GHCACHER_LIVE_BUDGET_S:-10}" ghcacher_live_smoke "$@"

PROGRAM="$V6/dl/fixtures/ghcacher_live.dl6"
ENDPOINT="repos/cli/cli"
PORT="${GHCACHER_LIVE_PORT:-17583}"
BASE="http://127.0.0.1:$PORT"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/ghcacher-live.XXXXXX")"
SERVER_PID=""

stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
cleanup() { stop_server; rm -rf -- "$WORK"; }
trap cleanup EXIT

skip() { printf 'SKIP  ghcacher live smoke: %s\n' "$*"; exit 0; }
fail() { printf 'FAIL  ghcacher live smoke: %s\n' "$*"; [ -s "$WORK/server.log" ] && tail -15 "$WORK/server.log"; exit 1; }

command -v gh >/dev/null 2>&1 || skip "gh is not on PATH"
command -v jq >/dev/null 2>&1 || skip "jq is not on PATH"
command -v swipl >/dev/null 2>&1 || skip "swipl is not on PATH"
gh auth status >/dev/null 2>&1 || skip "gh auth status failed; no keychain credentials"
run_capped 3 gh api rate_limit >"$WORK/rate.json" 2>/dev/null || skip "api.github.com unreachable"
QUOTA_BEFORE="$(jq -r '.rate.remaining' "$WORK/rate.json" 2>/dev/null || echo 0)"

# gh's exit status on a 304 is undocumented and every conditional template
# depends on it being nonzero, so it is measured here rather than assumed.
COND_TAG="$(run_capped 4 gh api --include "$ENDPOINT" 2>/dev/null | grep -i '^etag:' | head -1 | sed -e 's/^[^:]*:[[:space:]]*//' -e 's/\r$//')"
[ -n "$COND_TAG" ] || skip "no ETag on an unconditional $ENDPOINT call"
run_capped 4 gh api --include -H "If-None-Match: $COND_TAG" "$ENDPOINT" >/dev/null 2>&1
GH_304_EXIT=$?

# The goldens' compile door.
swipl -q -l "$V6/prolog/compile.pl" \
  -g "compile_dl6('$PROGRAM', '$WORK/ghcacher_live.ts')" \
  -g halt >"$WORK/compile.log" 2>&1 \
  || fail "compile_dl6 refused the fixture: $(tail -3 "$WORK/compile.log")"

TSV2_DB="file:$WORK/live.sqlite" TSV2_PORT="$PORT" \
  node --experimental-transform-types "$TSV2/serve/main.ts" >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
ready=0
for _ in $(seq 1 100); do
  capped_curl 2 -s -o /dev/null "$BASE/stats" 2>/dev/null && { ready=1; break; }
  kill -0 "$SERVER_PID" 2>/dev/null || fail "server died on boot: $(tail -5 "$WORK/server.log")"
  sleep 0.05
done
[ "$ready" = 1 ] || fail "server did not answer on port $PORT"

status="$(capped_curl 6 -s -o "$WORK/load.json" -w '%{http_code}' -X POST "$BASE/program" \
  -H 'content-type: text/plain' --data-binary "@$PROGRAM")"
[ "$status" = "200" ] || fail "POST /program -> $status $(cat "$WORK/load.json")"
# An empty literal list is a bind plan with no timer, which polls nothing and
# would still answer 200 here.
jq -e '.binds[] | select(.name == "interval") | .literals | length > 0' "$WORK/load.json" >/dev/null \
  || fail "the loaded program spins no interval timer: $(cat "$WORK/load.json")"

# The endpoint set is data, so the org tripwire is one more arrival row on the
# same rels rather than a second program.
batch="{\"rel\":\"watch_endpoint\",\"sign\":\"add\",\"row\":[\"$ENDPOINT\"]}"
ORG_ENDPOINT=""
if [ -n "${GHCACHER_ORG:-}" ]; then
  ORG_ENDPOINT="orgs/${GHCACHER_ORG}/events"
  batch="$batch,{\"rel\":\"watch_endpoint\",\"sign\":\"add\",\"row\":[\"$ORG_ENDPOINT\"]}"
fi
capped_curl 3 -s -o /dev/null -X POST "$BASE/arrivals" -d "{\"batch\":[$batch]}" \
  || fail "POST /arrivals failed"

calls_for() {
  capped_curl 2 -s "$BASE/idb/call_log" | jq -c --arg ep "$1" '[.rows[] | select(.[0] == $ep)]' 2>/dev/null
}
cache_for() {
  capped_curl 2 -s "$BASE/idb/cached_body" | jq -S -c --arg ep "$1" '[.rows[] | select(.[0] == $ep)]' 2>/dev/null
}

await_calls() {
  local want="$1" deadline=$((SECONDS + 7)) rows
  while [ "$SECONDS" -lt "$deadline" ]; do
    rows="$(calls_for "$ENDPOINT")"
    [ -n "$rows" ] && [ "$(printf '%s' "$rows" | jq 'length')" -ge "$want" ] && { printf '%s' "$rows"; return 0; }
    sleep 0.2
  done
  return 1
}

await_calls 1 >/dev/null || fail "no call landed within 7s (log: $(tail -3 "$WORK/server.log"))"
CACHE_AFTER_200="$(cache_for "$ENDPOINT")"
polled="$(await_calls 2)" || fail "only one call landed; the etag latch never re-minted the witness"
CACHE_AFTER_304="$(cache_for "$ENDPOINT")"

first_status="$(printf '%s' "$polled" | jq -r '.[0][2]')"
first_hit="$(printf '%s' "$polled" | jq -r '.[0][3]')"
first_left="$(printf '%s' "$polled" | jq -r '.[0][4]')"
second_status="$(printf '%s' "$polled" | jq -r '.[1][2]')"
second_hit="$(printf '%s' "$polled" | jq -r '.[1][3]')"
second_left="$(printf '%s' "$polled" | jq -r '.[1][4]')"

[ "$first_status" = "200" ] || fail "poll 1 answered $first_status, expected 200"
[ "$first_hit" = "0" ] || fail "poll 1 logged cache_hit=$first_hit, expected 0"
[ "$second_status" = "304" ] || fail "poll 2 answered $second_status, expected 304"
[ "$second_hit" = "1" ] || fail "poll 2 logged cache_hit=$second_hit, expected 1"

# Row-set equality on the cache, not a row count: the defect plan 1.1 measured
# destroyed the body while leaving the row shape intact.
[ "$(printf '%s' "$CACHE_AFTER_200" | jq 'length')" = "1" ] \
  || fail "cached_body holds $(printf '%s' "$CACHE_AFTER_200" | jq 'length') rows for $ENDPOINT after the 200, expected 1"
[ "$(printf '%s' "$CACHE_AFTER_200" | jq -r '.[0][1] | length')" -gt 100 ] \
  || fail "cached_body body is empty after the 200"
[ "$CACHE_AFTER_200" = "$CACHE_AFTER_304" ] \
  || fail "the 304 moved cached_body; a conditional confirmation is zero public delta"

# Q3, the private-org blindness tripwire. Printed, never asserted.
if [ -n "$ORG_ENDPOINT" ]; then
  org_rows="$(calls_for "$ORG_ENDPOINT")"
  org_status="$(printf '%s' "$org_rows" | jq -r '.[0][2] // "none"')"
  org_events="$(cache_for "$ORG_ENDPOINT" | jq -r '.[0][1] // "[]"' | jq 'length' 2>/dev/null || echo 0)"
  case "$org_status" in
    200)  printf 'ORG   %s -> 200, %s event rows visible to this token\n' "$ORG_ENDPOINT" "$org_events" ;;
    304)  printf 'ORG   %s -> 304, unchanged since the prior tag, so the feed is visible\n' "$ORG_ENDPOINT" ;;
    404)  printf 'ORG   %s -> 404, BLIND: this token cannot see the org events feed\n' "$ORG_ENDPOINT" ;;
    none) printf 'ORG   %s -> no call landed inside the budget\n' "$ORG_ENDPOINT" ;;
    *)    printf 'ORG   %s -> %s\n' "$ORG_ENDPOINT" "$org_status" ;;
  esac
fi

printf 'PASS  ghcacher live smoke: %s 200->304, gh 304 exit=%s, quota %s before, %s left after the 200, %s left after the 304, cached_body byte-identical across the 304\n' \
  "$ENDPOINT" "$GH_304_EXIT" "$QUOTA_BEFORE" "$first_left" "$second_left"
