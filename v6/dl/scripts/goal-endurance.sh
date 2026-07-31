#!/usr/bin/env bash
# THE END GOAL, EXECUTABLE (user ask 2026-07-27: "define it in bash so we know
# it works").
#
#   A dl program's time-delayed derivations are DURABLE. A value that is still
#   "in the future" when the server process dies is delivered after reboot,
#   exactly once. No await lives in process memory; every yield point is a row
#   (demand row -> effect_cache witness -> response commit). Kill -9 at any
#   point in that chain and the chain finishes on the next boot.
#
# Phases:
#   0  liveness   : sh-host with a real wall-clock delay delivers while up
#   1  endurance  : kill -9 mid-delay; reboot on the same db; value arrives
#   2  exactly-once: another reboot; no re-fire (witness cache holds); count==1
#
# Exit 0 = the end goal exists. Any phase failing prints WHERE the chain broke.
# BUDGET (timeout-gun lane, 2026-07-31). Measured wall: 17s at the default
# DL_GOAL_NAP of 4. Default 600s is ~35x that; the extra room is because the
# nap is an argument and a longer nap is a legitimate run. Whole-script cap:
# the cost is three server generations, two of which this script kills -9
# itself, so an orphan from a botched kill is exactly the shape a per-command
# cap would miss. Every curl also carries ENDURANCE_HTTP_BUDGET_S.
# Override with DL_ENDURANCE_BUDGET_S.
set -uo pipefail

. "$(cd "$(dirname "$0")/../../tools" && pwd)/run-capped.sh"
cap_self "${DL_ENDURANCE_BUDGET_S:-600}" dl_endurance "$@"

cd "$(dirname "$0")/.."

PORT="${DL_GOAL_PORT:-17491}"
NAP="${DL_GOAL_NAP:-4}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/dl-goal.XXXXXX")"
DB="$WORK/endure.sqlite"
BASE="http://127.0.0.1:$PORT"
SERVER_PID=""
PASS_LINES=()

say()  { printf '\n== %s\n' "$*"; }
pass() { PASS_LINES+=("PASS  $*"); }
fail() { printf 'FAIL  %s\n' "$*"; stop_server; exit 1; }

start_server() {
  local started=0
  DL_DB_PATH="$DB" DL_PORT="$PORT" node --experimental-transform-types src/main.ts \
    >"$WORK/server.log" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 50); do
    if capped_curl "${ENDURANCE_HTTP_BUDGET_S:-30}" -sf "$BASE/idb/nothing" >/dev/null 2>&1 || capped_curl "${ENDURANCE_HTTP_BUDGET_S:-30}" -s -o /dev/null "$BASE/query" 2>/dev/null; then
      started=1
      break
    fi
    kill -0 "$SERVER_PID" 2>/dev/null || fail "server died on boot: $(tail -3 "$WORK/server.log")"
    sleep 0.2
  done
  [ "$started" = "1" ] || fail "server did not become ready on port $PORT: $(tail -3 "$WORK/server.log")"
}

stop_server() { [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null; wait "$SERVER_PID" 2>/dev/null; SERVER_PID=""; }
trap stop_server EXIT

post_program() {
  # want(tag, salt) is the demand seed; napper is a sh host that sleeps (a real
  # wall-clock yield) then emits its value; woke is the delayed derivation.
  local resp
  resp=$(capped_curl "${ENDURANCE_LOAD_BUDGET_S:-900}" -s -w '\n%{http_code}' -X POST "$BASE/edb/program" \
    -H 'content-type: text/plain' --data-binary @- <<DL
rel want(tag: text, salt: text).
rel woke(tag: text, salt: text, value: text).

sh napper(tag: text, value: text) =
  \`sleep $NAP; printf '{"value":"awake-%s"}\n' {tag}\`.

woke(tag, salt, value) <- want(tag, salt), napper?(tag, salt, value).
DL
)
  local code="${resp##*$'\n'}"
  [ "$code" = "200" ] || fail "POST /edb/program -> $code: $(printf '%s' "$resp" | head -3)"
}

plant() { # plant <tag>
  local resp code
  resp=$(capped_curl "${ENDURANCE_HTTP_BUDGET_S:-30}" -s -w '\n%{http_code}' -X POST "$BASE/edb/want" \
    -H 'content-type: application/json' \
    -d "{\"rows\":[{\"tag\":\"$1\",\"salt\":\"s-$1\"}]}")
  code="${resp##*$'\n'}"
  [ "$code" = "200" ] || fail "POST /edb/want($1) -> $code: $(printf '%s' "$resp" | head -3)"
}

woke_count() { # woke_count <tag>
  capped_curl "${ENDURANCE_HTTP_BUDGET_S:-30}" -s "$BASE/idb/woke" | grep -o "awake-$1" | wc -l | tr -d ' '
}

wait_woke() { # wait_woke <tag> <seconds>
  local deadline=$((SECONDS + $2))
  while [ $SECONDS -lt $deadline ]; do
    [ "$(woke_count "$1")" -ge 1 ] && return 0
    sleep 0.5
  done
  return 1
}

say "phase 0: liveness (delay=$NAP s, db=$DB)"
start_server
post_program
plant alpha
[ "$(woke_count alpha)" = "0" ] || fail "phase 0: woke(alpha) arrived before the delay elapsed"
wait_woke alpha $((NAP + 6)) || fail "phase 0: woke(alpha) never arrived while the server stayed up (log: $WORK/server.log)"
pass "phase 0: delayed value delivered while up"

say "phase 1: endurance (kill -9 mid-delay, reboot, same db)"
plant bravo
sleep 1   # inside the nap: the demand row + in-flight witness are on disk, the value is not
[ "$(woke_count bravo)" = "0" ] || fail "phase 1: bravo arrived too early to test anything"
stop_server
pass "phase 1: server killed mid-delay"
start_server
post_program   # program re-POST is part of the contract until programs self-rehydrate
wait_woke bravo $((NAP + 10)) || fail "phase 1 IS THE GAP: woke(bravo) never arrived after reboot -- the in-flight effect_cache row from the killed run wedges the demand instead of re-firing (log: $WORK/server.log)"
pass "phase 1: value planted before the crash arrived after it"

say "phase 2: exactly-once across a third boot"
stop_server
start_server
post_program
sleep $((NAP + 2))
alpha_n=$(woke_count alpha); bravo_n=$(woke_count bravo)
[ "$alpha_n" = "1" ] || fail "phase 2: woke(alpha) count=$alpha_n, want 1 (witness cache must hold across boots)"
[ "$bravo_n" = "1" ] || fail "phase 2: woke(bravo) count=$bravo_n, want 1"
pass "phase 2: both values exactly once after reboot #2"

for pass_line in "${PASS_LINES[@]}"; do
  printf '%s\n' "$pass_line"
done

say "END GOAL HOLDS: time flows through rows, not process memory. ($WORK kept for autopsy)"
