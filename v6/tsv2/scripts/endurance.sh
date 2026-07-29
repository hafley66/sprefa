#!/usr/bin/env bash
# endurance.sh — RECEIPT (5) of the runtime-bridge arc
# (plans/2026-07-29-runtime-bridge-header.md scope 5): the served tsv2 engine's
# effects are durable, and an ANSWERED witness never fires again.
#
# The chain has no await living in process memory: a demand row is a SQLite row,
# a witness's state is a SQLite row (`__host_witness`), and a response is an
# ordinary EDB arrival. So `kill -9` at any point and the chain finishes on the
# next boot -- and the parts that already finished stay finished.
#
# Four server generations on ONE file db. `marks` counts SPAWNS STARTED (the
# host appends its tag before it sleeps, see served-endurance.dl6):
#
#   A  nap 0   seed quick            -> woke 1, marks 1
#   B  nap 8   boot replay only      -> marks STILL 1   (answered: no refire)
#      then    seed slow, kill -9 mid-nap
#   C  nap 0   boot replay           -> woke 2, marks 3 (unanswered: finishes)
#   D  nap 0   boot replay only      -> woke 2, marks 3 (nothing refires)
#
# The step from 1 to 3 marks is the honest boundary and is asserted, not hidden:
# an UNANSWERED witness is at-least-once (its killed spawn already counted), an
# ANSWERED one is exactly-once. That is the whole law.
#
# SABOTAGE RECEIPT: making serve/1_hosts.ts's boot replay ignore the durable
# answered set (treat every demand row as unanswered) fails phase B with
# "marks 2, expected 1".
set -uo pipefail
cd "$(dirname "$0")/.."

PORT="${TSV2_ENDURANCE_PORT:-17561}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-endure.XXXXXX")"
DB="file:$WORK/endure.sqlite"
MARKS="$WORK/marks"
PROGRAM="../dl/fixtures/served-endurance.dl6"
BASE="http://127.0.0.1:$PORT"
SERVER_PID=""
: >"$MARKS"

fail() { printf 'FAIL  %s\n' "$*"; stop_server; exit 1; }

start_server() {
  local nap="$1"
  TSV2_DB="$DB" TSV2_PORT="$PORT" TSV2_MARKS="$MARKS" TSV2_NAP="$nap" \
    node --experimental-transform-types serve/main.ts >>"$WORK/server.log" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 60); do
    if curl -s -o /dev/null "$BASE/ticks" 2>/dev/null; then return; fi
    kill -0 "$SERVER_PID" 2>/dev/null || fail "server died on boot: $(tail -5 "$WORK/server.log")"
    sleep 0.2
  done
  fail "server did not become ready on port $PORT"
}

stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap stop_server EXIT

load_program() {
  local code
  code=$(curl -s -o "$WORK/load.json" -w '%{http_code}' -X POST "$BASE/program" \
    -H 'content-type: text/plain' --data-binary "@$PROGRAM")
  [ "$code" = "200" ] || fail "POST /program -> $code $(cat "$WORK/load.json")"
}

post_seed() {
  curl -s -o /dev/null -X POST "$BASE/arrivals" \
    -d "{\"batch\":[{\"rel\":\"seed\",\"sign\":\"add\",\"row\":[\"$1\"]}]}"
}

woke_count() {
  curl -s "$BASE/idb/woke" | node -e 'let t="";process.stdin.on("data",c=>t+=c).on("end",()=>process.stdout.write(String(JSON.parse(t).rows.length)))'
}

marks_count() { /usr/bin/grep -c . "$MARKS" 2>/dev/null || echo 0; }

wait_for_woke() {
  local want="$1" deadline=$((SECONDS + 30))
  while [ "$(woke_count)" != "$want" ]; do
    [ "$SECONDS" -ge "$deadline" ] && fail "timeout waiting for woke to reach $want (is $(woke_count), marks $(marks_count))"
    sleep 0.2
  done
}

expect_marks() {
  local want="$1" phase="$2"
  local have
  have="$(marks_count)"
  [ "$have" = "$want" ] || fail "phase $phase: marks $have, expected $want"
}

# ── A: liveness ──────────────────────────────────────────────────────────────
start_server 0
load_program
post_seed quick
wait_for_woke 1
expect_marks 1 A
printf 'PASS  A liveness: the host ran and its value landed (woke 1, marks 1)\n'
stop_server

# ── B: an ANSWERED witness does not refire across a restart ──────────────────
start_server 8
load_program
sleep 1
[ "$(woke_count)" = "1" ] || fail "phase B: woke $(woke_count), expected 1 after boot replay"
expect_marks 1 B
printf 'PASS  B exactly-once: boot replay did NOT refire the answered witness (marks still 1)\n'

# kill -9 mid-nap, with the slow witness demanded but unanswered
post_seed slow
for _ in $(seq 1 40); do
  [ "$(marks_count)" = "2" ] && break
  sleep 0.2
done
expect_marks 2 B2
kill -9 "$SERVER_PID" 2>/dev/null
wait "$SERVER_PID" 2>/dev/null
SERVER_PID=""
[ "$(woke_count 2>/dev/null)" = "" ] || true
printf 'PASS  B2 crash: killed mid-nap with one demanded, unanswered witness (marks 2)\n'

# ── C: the unanswered witness finishes on the next boot ──────────────────────
start_server 0
load_program
wait_for_woke 2
expect_marks 3 C
printf 'PASS  C endurance: the crashed witness re-ran and its value landed (woke 2, marks 3)\n'
stop_server

# ── D: nothing refires once everything is answered ───────────────────────────
start_server 0
load_program
sleep 1
[ "$(woke_count)" = "2" ] || fail "phase D: woke $(woke_count), expected 2"
expect_marks 3 D
printf 'PASS  D exactly-once: a second restart refired nothing (woke 2, marks 3)\n'
stop_server

printf '\nENDURANCE RECEIPT HOLDS (db %s)\n' "$DB"
