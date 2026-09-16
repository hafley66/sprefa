#!/usr/bin/env bash
# receipts.sh -- the teardown-and-flatten lab's runnable evidence.
#
# `bash receipts.sh` exits 0 when every receipt below matched its declared
# expectation, nonzero otherwise. Each receipt prints what it measured, so a
# reader can check the claim rather than trust the exit code.
#
# HERMETIC, the same way rxoracle is: SPREFA_CONFIG points at a path that does
# not exist, DL_NO_DAEMON=1, the db is :memory:, the port is ephemeral, and
# every scratch file lives under one mktemp -d removed on exit. No daemon is
# contacted and nothing under ~/.local/state is read or written.
#
# THE RECEIPTS
#
#   R1  the teardown SIGNAL already exists: a superseded demand row produces a
#       `del` on `__host_demand_<name>` in the served tick stream, at the tick
#       the supersession happens, carrying the witness digest.
#   R2  nothing reads it: the superseded subprocess still runs to completion and
#       its answer still lands durably, two ticks after it stopped being wanted.
#       The program's own relation is nevertheless CORRECT.
#   R3  the four flatteners are four operators over that one signal. Same
#       program, same arrivals, same spawn shape, one operator changed, four
#       different ledgers.
#   R4  `concat` reproduces the shipped runner's ledger exactly, which is what
#       makes R3's other three credible.
#   R5  the concatMap serialization is a SEPARATE cost from the cancel question:
#       two demands that never compete, with no retraction anywhere, are still
#       serialized.
#   R6  the winner's add and the loser's del are on ONE tick, where the log is a
#       set. Both intra-tick sign orders are faithful readings and they produce
#       different ledgers, so the order is a free choice (verdict card 4).
#   R7  `finalize/1` already observes the supersession that a live
#       `unsubscribe/1` would observe, with no effect plane (verdict card 6a).

set -uo pipefail

LAB_DIR="$(cd "$(dirname "$0")" && pwd)"
TSV2_DIR="$(cd "$LAB_DIR/../../v6/tsv2" && pwd)"
NODE_RUN=(node --experimental-transform-types)

WORK="$(mktemp -d "${TMPDIR:-/tmp}/teardown-lab.XXXXXX")"
SERVER_PID=""
PROBE_PID=""
CAPTURE_PID=""
FAILURES=0

cleanup() {
  for pid in "$PROBE_PID" "$CAPTURE_PID" "$SERVER_PID"; do
    [ -n "$pid" ] && kill -9 "$pid" 2>/dev/null
  done
  wait 2>/dev/null
  rm -rf "$WORK"
}
trap cleanup EXIT

say() { printf '%s\n' "$*"; }
loud() { printf '\n== %s ==\n' "$*"; }

check() { # check <label> <expected> <actual>
  if [ "$2" = "$3" ]; then
    say "  PASS  $1"
    return 0
  fi
  say "  FAIL  $1"
  say "        expected: $2"
  say "        actual:   $3"
  FAILURES=$((FAILURES + 1))
  return 1
}

# Boot one served process on an ephemeral port with the given program.
# Echoes the port. Sets SERVER_PID.
boot() { # boot <program-file> <marks-file> <nap>
  local program="$1" marks="$2" nap="$3" attempt port=""
  : >"$marks"
  ( cd "$TSV2_DIR" && env SPREFA_CONFIG=/nonexistent/teardown-lab.toml DL_NO_DAEMON=1 \
      RXO_MARKS="$marks" RXO_NAP="$nap" \
      "${NODE_RUN[@]}" cli/bop.ts serve --port 0 --db ":memory:" ) \
    >"$WORK/serve.log" 2>&1 &
  SERVER_PID=$!
  for attempt in $(seq 1 150); do
    port="$(sed -n 's/.*serving on \([0-9]*\).*/\1/p' "$WORK/serve.log" | head -1)"
    [ -n "$port" ] && break
    sleep 0.1
  done
  [ -n "$port" ] || { say "  server never listened:"; sed -n 1,20p "$WORK/serve.log"; return 1; }
  local loaded
  loaded="$(curl -s -X POST --data-binary @"$program" "http://127.0.0.1:$port/program")"
  case "$loaded" in
    *'"loaded":true'*) : ;;
    *) say "  program not loaded: $loaded"; return 1 ;;
  esac
  printf '%s' "$port"
}

halt() {
  for pid in "$PROBE_PID" "$CAPTURE_PID" "$SERVER_PID"; do
    [ -n "$pid" ] && kill -9 "$pid" 2>/dev/null
  done
  PROBE_PID=""; CAPTURE_PID=""; SERVER_PID=""
  wait 2>/dev/null
}

FLAGSHIP="$TSV2_DIR/rxoracle/cases/switchmap_inner_in_flight/leg-b.dl6"

command -v jq >/dev/null || { say "jq is required"; exit 2; }
command -v curl >/dev/null || { say "curl is required"; exit 2; }

# The probe imports the bare specifier `rxjs`, and node resolves that by walking
# up from the PROBE'S OWN directory, which is outside any package. rxoracle's
# leg-A files get theirs for free by living under v6/tsv2. This symlink is the
# same resolution by hand; it is not an import of this repository (the probe
# still names no module from it) and `check-imports` does not scan labs/.
[ -e "$LAB_DIR/node_modules" ] || ln -s "$TSV2_DIR/node_modules" "$LAB_DIR/node_modules"
[ -d "$TSV2_DIR/node_modules/rxjs" ] || { say "run pnpm install in $TSV2_DIR first"; exit 2; }

# ─────────────────────────────────────────────────────────────────────────────
loud "R1 + R2  the teardown signal exists and nothing reads it"
# ─────────────────────────────────────────────────────────────────────────────
say "One session, routed to r1 then to r2 while r1's fetch is still in flight."
say "The program is rxoracle's flagship case, byte-unmodified."

PORT="$(boot "$FLAGSHIP" "$WORK/marks1" 1.95)" || exit 1
( curl -sN "http://127.0.0.1:$PORT/ticks" >"$WORK/capture" ) & CAPTURE_PID=$!
sleep 0.5
curl -s -X POST -d '{"batch":[{"rel":"route_change","sign":"add","row":["s1","r1"]}]}' \
  "http://127.0.0.1:$PORT/arrivals" >/dev/null
sleep 1
curl -s -X POST -d '{"batch":[{"rel":"route_change","sign":"add","row":["s1","r2"]}]}' \
  "http://127.0.0.1:$PORT/arrivals" >/dev/null
sleep 4
RESPONSE_ROWS="$(curl -s "http://127.0.0.1:$PORT/idb/__host_response_fetch_body" | jq -r '.rows | length')"
BODY_ROWS="$(curl -s "http://127.0.0.1:$PORT/idb/body" | jq -r '.rows | length')"
halt

say ""
say "  the served tick stream, verbatim:"
sed 's/^/    /' "$WORK/capture" | grep -v '^ *$'

# The demand rel's `del` half, read out of the capture with jq rather than a
# regex: the tick log line IS json, so parsing it as json is the honest read.
sed -n 's/^data: //p' "$WORK/capture" >"$WORK/ticks.jsonl"
DEMAND_DEL_TICK="$(jq -r --slurp \
  '[.[] | select(.deltas.__host_demand_fetch_body.del // [] | length > 0) | .tick] | first // "none"' \
  "$WORK/ticks.jsonl")"
DEMAND_DEL_WITNESS="$(jq -r --slurp \
  '[.[] | .deltas.__host_demand_fetch_body.del // [] | .[] | .[1]] | first // "none"' \
  "$WORK/ticks.jsonl")"
# The response rel's add for the RETRACTED witness, and the tick it landed on:
# the "ran to completion after it stopped being wanted" half of R2, as an event
# rather than as a row count.
DEAD_ANSWER_TICK="$(jq -r --slurp \
  '[.[] | select([.deltas.__host_response_fetch_body.add // [] | .[] | .[0]]
       | index("witness|fetch_body|route_id:text=r1")) | .tick] | first // "none"' \
  "$WORK/ticks.jsonl")"

say ""
check "R1  a superseded demand row produces a del delta" \
  "tick 3" "tick ${DEMAND_DEL_TICK:-none}"
check "R1  that del carries the retracted route's own witness digest" \
  "witness|fetch_body|route_id:text=r1" "${DEMAND_DEL_WITNESS:-none}"

say ""
say "  the shipped runner's spawn ledger:"
sed 's/^/    /' "$WORK/marks1"
MARKS1="$(tr '\n' '/' <"$WORK/marks1")"
check "R2  the superseded r1 still ran to completion" \
  "start r1/done r1/start r2/done r2/" "$MARKS1"
check "R2  the dead inner's answer landed TWO ticks after its demand retracted" \
  "tick 5" "tick ${DEAD_ANSWER_TICK:-none}"
check "R2  the dead inner's answer is stored durably anyway" "2" "$RESPONSE_ROWS"
check "R2  the visible relation is nevertheless CORRECT (one row, r2)" "1" "$BODY_ROWS"

# ─────────────────────────────────────────────────────────────────────────────
loud "R3 + R4  four flatteners, one signal"
# ─────────────────────────────────────────────────────────────────────────────
say "Identical program, identical arrivals, identical spawn shape. The probe"
say "(labs/teardown-flatten/flatten-probe.ts) reads the SAME /ticks stream and"
say "runs its own effects beside the engine's, changing only the rx operator."
say "It imports rxjs and node builtins and nothing from this repository."

run_flattener() { # run_flattener <label> <program> <input-columns> <flattener> <sign-order>
  local name="$1" program="$2" columns="$3" flattener="$4" order="$5" port
  port="$(boot "$program" "$WORK/marks-$name" 1.95)" || return 1
  ( cd "$LAB_DIR" && "${NODE_RUN[@]}" flatten-probe.ts \
      "$port" fetch_body "$columns" "$WORK/probe-$name" 1.95 "$flattener" "$order" ) \
    >"$WORK/probe-$name.log" 2>&1 &
  PROBE_PID=$!
  sleep 0.6
  curl -s -X POST -d '{"batch":[{"rel":"route_change","sign":"add","row":["s1","r1"]}]}' \
    "http://127.0.0.1:$port/arrivals" >/dev/null
  sleep 1
  curl -s -X POST -d '{"batch":[{"rel":"route_change","sign":"add","row":["s1","r2"]}]}' \
    "http://127.0.0.1:$port/arrivals" >/dev/null
  sleep 4
  halt
}

run_flattener concat  "$FLAGSHIP" 1 concat  add-first || exit 1
run_flattener merge   "$FLAGSHIP" 1 merge   add-first || exit 1
run_flattener switch  "$FLAGSHIP" 1 switch  add-first || exit 1
run_flattener switch-del-first "$FLAGSHIP" 1 switch del-first || exit 1
run_flattener exhaust "$LAB_DIR/exhaust-case.dl6" 2 exhaust add-first || exit 1

say ""
printf '  %-17s %s\n' "flattener" "ledger"
printf '  %-17s %s\n' "---------" "------"
for flattener in concat merge switch switch-del-first exhaust; do
  printf '  %-17s %s\n' "$flattener" "$(tr '\n' '/' <"$WORK/probe-$flattener")"
done

say ""
check "R4  probe 'concat' reproduces the shipped runner's ledger" \
  "start r1/done r1/start r2/done r2/" "$(tr '\n' '/' <"$WORK/probe-concat")"
check "R3  'merge' overlaps the two inners" \
  "start r1/start r2/done r1/done r2/" "$(tr '\n' '/' <"$WORK/probe-merge")"
check "R3  'switch' tears the loser down on its own del and the winner completes" \
  "start r1/start r2/torn down r1/done r2/" "$(tr '\n' '/' <"$WORK/probe-switch")"
check "R3  'exhaust' drops the second demand while the slot is busy" \
  "start s1-r1/done s1-r1/" "$(tr '\n' '/' <"$WORK/probe-exhaust")"

# ─────────────────────────────────────────────────────────────────────────────
# R6, which was an author's wrong prediction before it was a receipt. The
# expectation written first was `start r1/torn down r1/start r2/done r2` -- the
# teardown BEFORE the winner starts. It is not what happens, and the reason is
# not a bug in the probe: the winner's `add` and the loser's `del` are on ONE
# tick (R1 measured both on tick 3), and a tick's delta halves are a SET, so
# nothing in the data orders them. The probe took `add` first; taking `del`
# first is equally faithful to the same tick log and yields a different ledger.
# ─────────────────────────────────────────────────────────────────────────────
say ""
check "R6  reading the same tick del-first tears down BEFORE the winner starts" \
  "start r1/torn down r1/start r2/done r2/" "$(tr '\n' '/' <"$WORK/probe-switch-del-first")"
say "  R6  both ledgers above are faithful readings of the SAME tick stream."
say "      The intra-tick sign order is a free choice the tick log does not make."

# ─────────────────────────────────────────────────────────────────────────────
loud "R5  serialization is a separate cost from cancellation"
# ─────────────────────────────────────────────────────────────────────────────
say "Two jobs demanded in ONE batch. They share no key, nothing retracts, and"
say "no supersession is possible. The concatMap is still the whole cost."

PORT="$(boot "$TSV2_DIR/rxoracle/cases/host_concurrency/leg-b.dl6" "$WORK/marks5" 1.95)" || exit 1
SERIAL_START="$(date +%s)"
curl -s -X POST -d '{"batch":[{"rel":"job","sign":"add","row":["j1"]},{"rel":"job","sign":"add","row":["j2"]}]}' \
  "http://127.0.0.1:$PORT/arrivals" >/dev/null
for attempt in $(seq 1 120); do
  [ "$(curl -s "http://127.0.0.1:$PORT/idb/result" | jq -r '.rows | length')" = "2" ] && break
  sleep 0.25
done
SERIAL_SECONDS=$(( $(date +%s) - SERIAL_START ))
halt

say ""
say "  spawn ledger:"
sed 's/^/    /' "$WORK/marks5"
say "  wall seconds to both answers: ${SERIAL_SECONDS}s (two 1.95s jobs)"
check "R5  two independent jobs are serialized, never interleaved" \
  "start j1/done j1/start j2/done j2/" "$(tr '\n' '/' <"$WORK/marks5")"
if [ "$SERIAL_SECONDS" -ge 3 ]; then
  say "  PASS  R5  wall time is the SUM of the two jobs, not the max"
else
  say "  FAIL  R5  expected >=3s (serialized); got ${SERIAL_SECONDS}s"
  FAILURES=$((FAILURES + 1))
fi

# ─────────────────────────────────────────────────────────────────────────────
loud "R7  the reserved word 'unsubscribe' has a live synonym already"
# ─────────────────────────────────────────────────────────────────────────────
say "Card 6a's argument, as a program. finalize/1 is live and observes the same"
say "supersession an unsubscribe arm would observe -- with no host, no effect"
say "plane, and no teardown existing. See finalize-already-observes.dl6."

PORT="$(boot "$LAB_DIR/finalize-already-observes.dl6" "$WORK/marks7" 1)" || exit 1
curl -s -X POST -d '{"batch":[{"rel":"route_change","sign":"add","row":["s1","r1"]}]}' \
  "http://127.0.0.1:$PORT/arrivals" >/dev/null
curl -s -X POST -d '{"batch":[{"rel":"route_change","sign":"add","row":["s1","r2"]}]}' \
  "http://127.0.0.1:$PORT/arrivals" >/dev/null
ABANDONED="$(curl -s "http://127.0.0.1:$PORT/idb/abandoned" | jq -c '.rows')"
SURVIVING="$(curl -s "http://127.0.0.1:$PORT/idb/open_route" | jq -c '.rows')"
halt

say ""
check "R7  finalize derives the abandoned route today" '[["r1"]]' "$ABANDONED"
check "R7  and the keyed head holds only the winner" '[["s1","r2"]]' "$SURVIVING"

# ─────────────────────────────────────────────────────────────────────────────
loud "teardown-flatten receipts"
# ─────────────────────────────────────────────────────────────────────────────
if [ "$FAILURES" != "0" ]; then
  printf 'TEARDOWN LAB RED: %d receipt(s) did not match\n' "$FAILURES"
  exit 1
fi
printf 'TEARDOWN LAB HOLDS: every receipt as declared\n'
exit 0
