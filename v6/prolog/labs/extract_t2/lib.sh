#!/usr/bin/env bash
# lib.sh : the two doors, shared by every receipt in this lab.
#
# Sourced, never run. Provides:
#   oracle_log <program.dl6> <schedule.json>   -- the reference engine
#   served_log <program.dl6> <schedule.json>   -- the real emitted engine over HTTP
#   sequence                                    -- tick log -> per-rel delta sequence
#
# Both door helpers are lifted from v6/prolog/labs/rel_as_stream/receipts.sh,
# whose header states the ONE normalization and why it is honest: tick NUMBERS
# are not compared, each rel's own signed-row sequence is. The served engine and
# the schedule-fed oracle place drain ticks differently (runtime-bridge arc:
# "3 ticks fed vs 4 served, same deltas").
#
# THE ONE THING THIS LAB ADDS, and it is a finding rather than a convenience:
# the oracle leg does NOT use compile/scripts/dl6_oracle.pl. That door's
# schedule_value/2 maps every JSON string to an ATOM, and the reference engine
# has no JSON text parser (json-flex verdict section 5), so a schema DOCUMENT
# fed through it arrives as an opaque atom and every decode/2 silently derives
# nothing. t2_oracle.pl is dl6_oracle with one extra clause: a schedule value
# that is a nested JSON object or array becomes the braces TERM the engine
# actually destructures. See the verdict, finding D1.

set -u

T2_HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
T2_REPO="$(cd "$T2_HERE/../../../.." && pwd)"
T2_TSV2="$T2_REPO/v6/tsv2"

export SPREFA_CONFIG=/nonexistent/extract-t2.toml
export DL_NO_DAEMON=1

read -r -d '' T2_SEQUENCE_PY <<'PY' || true
import json, sys
order, sequences = [], {}
for raw in sys.stdin:
    raw = raw.strip()
    if raw.startswith("data: "):
        raw = raw[6:]
    if not raw:
        continue
    tick = json.loads(raw)
    for rel in sorted(tick.get("deltas", {})):
        delta = tick["deltas"][rel]
        if rel not in sequences:
            order.append(rel)
            sequences[rel] = []
        for row in delta.get("del", []):
            sequences[rel].append("%s - %s" % (rel, json.dumps(row, separators=(",", ":"), sort_keys=True)))
        for row in delta.get("add", []):
            sequences[rel].append("%s + %s" % (rel, json.dumps(row, separators=(",", ":"), sort_keys=True)))
for rel in sorted(order):
    for line in sequences[rel]:
        print(line)
PY

sequence() { python3 -c "$T2_SEQUENCE_PY"; }

oracle_log() {
  local program="$1" schedule="$2"
  ( cd "$T2_HERE" && swipl -q -l t2_oracle.pl -g "oracle('$program','$schedule')" -g halt )
}

served_log() {
  local program="$1" schedule="$2" settle="${3:-0.35}"
  local scratch port pid capture batch count index
  scratch="$(mktemp -d)"
  ( cd "$T2_TSV2" && node --experimental-transform-types cli/bop.ts \
      serve --port 0 --db ":memory:" >"$scratch/serve.log" 2>&1 ) &
  pid=$!
  port=""
  for _ in $(seq 1 300); do
    port="$(sed -n 's/.*serving on \([0-9]*\).*/\1/p' "$scratch/serve.log" | head -1)"
    [ -n "$port" ] && break
    sleep 0.1
  done
  if [ -z "$port" ]; then
    echo "SERVER NEVER LISTENED" >&2
    cat "$scratch/serve.log" >&2
    kill "$pid" 2>/dev/null; rm -rf "$scratch"; return 1
  fi
  local base="http://127.0.0.1:$port" loaded
  loaded="$(curl -s -X POST --data-binary @"$program" "$base/program")"
  case "$loaded" in
    *'"loaded":true'*) : ;;
    *) echo "LOAD FAILED: $loaded" >&2; kill "$pid" 2>/dev/null; rm -rf "$scratch"; return 1 ;;
  esac
  ( curl -sN "$base/ticks" >"$scratch/ticks" ) &
  capture=$!
  sleep 0.5
  count="$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))))' "$schedule")"
  for (( index = 0; index < count; index++ )); do
    batch="$(python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))[int(sys.argv[2])]))' "$schedule" "$index")"
    curl -s -X POST -d "{\"batch\":$batch}" "$base/arrivals" >/dev/null
    sleep "$settle"
  done
  sleep 1.0
  kill "$capture" 2>/dev/null
  kill "$pid" 2>/dev/null
  wait "$pid" 2>/dev/null
  cat "$scratch/ticks"
  rm -rf "$scratch"
}
