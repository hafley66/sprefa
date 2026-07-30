#!/usr/bin/env bash
#
# receipts.sh : rel-as-stream lab, the runnable half.
#
#   bash v6/prolog/labs/rel_as_stream/receipts.sh
#
# Exit 0 means every receipt held. Nonzero means at least one did not.
#
# Three legs:
#
#   1. the reference-engine receipts (0_receipts.pl, 12 of them)
#   2. TWO-DOOR grading: each .dl6 in this directory is run through the
#      reference engine (compile/scripts/dl6_oracle.pl) AND through the served
#      tsv2 engine (bop serve on an ephemeral port, driven by curl only), and
#      the two tick logs are compared
#   3. a durability receipt and a sabotage receipt
#
# Hermetic: SPREFA_CONFIG points at a path that does not exist, DL_NO_DAEMON=1,
# every server is :memory: except the durability case which uses a scratch file
# under mktemp. Nothing reads or writes ~/.local/state and no daemon is spoken
# to.
#
# THE ONE NORMALIZATION, and its reason. Tick NUMBERS are not compared; each
# REL's own sequence of signed rows is. The served engine and the schedule-fed
# oracle place drain ticks differently -- the runtime-bridge arc measured and
# recorded exactly this ("a carrying tick with an empty queue drains: 3 ticks
# fed vs 4 served, same deltas"), and v6/tsv2/rxoracle/README.md's N1 exists for
# the same reason. Inside one tick the log is already sorted by ticklog.ts and
# ticklog.pl, so concatenating ticks discards nothing that is graded per rel.
# Receipt (i) is the proof this comparison still discriminates.
#
# WHAT THIS NORMALIZATION GIVES UP, measured rather than assumed. Grouping by
# rel was not the first draft; the first draft compared one global sequence and
# case (d) went red on it. The diff was cross-rel INTERLEAVING only: the oracle
# fuses `ev +[4,"d"]` into the same tick that drains `evicted +[1,"a"]`, while
# the served engine drains the eviction in its own tick before the next arrival
# lands. Each rel's own sequence was identical on both doors. So cross-rel
# ORDER across a drain boundary is a function of when the arrival was posted
# relative to the drain, and it is not comparable between a schedule-fed door
# and a wall-clock-driven one. Stated here rather than quietly normalized away.

set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
TSV2="$REPO/v6/tsv2"
ORACLE_DIR="$REPO/v6/prolog/compile/scripts"

export SPREFA_CONFIG=/nonexistent/rel-as-stream.toml
export DL_NO_DAEMON=1

PASS=0
FAIL=0

ok()   { printf 'PASS %s\n' "$1"; PASS=$((PASS + 1)); }
bad()  { printf 'FAIL %s\n' "$1"; FAIL=$((FAIL + 1)); }

command -v swipl  >/dev/null || { echo "swipl is required";  exit 2; }
command -v curl   >/dev/null || { echo "curl is required";   exit 2; }
command -v python3>/dev/null || { echo "python3 is required"; exit 2; }

# ── the per-rel delta sequence, the thing actually compared ──────────────────

read -r -d '' SEQUENCE_PY <<'PY' || true
import json, sys
# stdin: tick-log lines (JSONL, optionally SSE "data: " prefixed).
# stdout: one "<rel> <sign> <row>" line per delta, grouped by rel, each rel's
# own sequence in tick order. Grouping is what makes cross-rel interleaving
# across a drain boundary a non-comparison; see the header.
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
            sequences[rel].append("%s - %s" % (rel, json.dumps(row, separators=(",", ":"))))
        for row in delta.get("add", []):
            sequences[rel].append("%s + %s" % (rel, json.dumps(row, separators=(",", ":"))))
for rel in sorted(order):
    for line in sequences[rel]:
        print(line)
PY

sequence() { python3 -c "$SEQUENCE_PY"; }

# ── leg: the reference engine over .dl6 text ─────────────────────────────────

oracle_log() {
  local program="$1" schedule="$2"
  ( cd "$ORACLE_DIR" && swipl -q -l dl6_oracle.pl \
      -g "oracle('$program','$schedule')" -g halt )
}

# ── leg: the served tsv2 engine, curl only ───────────────────────────────────

served_log() {
  local program="$1" schedule="$2" db="${3:-:memory:}"
  local scratch port pid capture batch count index
  scratch="$(mktemp -d)"
  ( cd "$TSV2" && node --experimental-transform-types cli/bop.ts \
      serve --port 0 --db "$db" >"$scratch/serve.log" 2>&1 ) &
  pid=$!
  port=""
  for _ in $(seq 1 200); do
    port="$(sed -n 's/.*serving on \([0-9]*\).*/\1/p' "$scratch/serve.log" | head -1)"
    [ -n "$port" ] && break
    sleep 0.1
  done
  if [ -z "$port" ]; then
    echo "SERVER NEVER LISTENED" >&2
    cat "$scratch/serve.log" >&2
    kill "$pid" 2>/dev/null
    rm -rf "$scratch"
    return 1
  fi
  local base="http://127.0.0.1:$port"
  local loaded
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
    sleep 0.35
  done
  sleep 0.8
  kill "$capture" 2>/dev/null
  kill "$pid" 2>/dev/null
  wait "$pid" 2>/dev/null
  cat "$scratch/ticks"
  rm -rf "$scratch"
}

two_door() {
  local name="$1" program="$HERE/$2" schedule="$HERE/$3"
  local scratch
  scratch="$(mktemp -d)"
  oracle_log "$program" "$schedule" | sequence >"$scratch/oracle"
  served_log "$program" "$schedule" | sequence >"$scratch/served"
  if [ ! -s "$scratch/oracle" ]; then
    bad "$name (oracle produced nothing)"
  elif diff -u "$scratch/oracle" "$scratch/served" >"$scratch/diff"; then
    ok "$name ($(wc -l <"$scratch/oracle" | tr -d ' ') deltas, both doors)"
  else
    bad "$name"
    sed -n 1,30p "$scratch/diff"
  fi
  rm -rf "$scratch"
}

echo "── leg 1: reference-engine receipts ─────────────────────────────────────"
if swipl -q -l "$HERE/0_receipts.pl" -g go -g halt; then
  ok "0_receipts.pl"
else
  bad "0_receipts.pl"
fi

echo
echo "── leg 2: two-door grading over the .dl6 corpus ─────────────────────────"

# (a) THE BUILD. A log rel carrying a surface ordinal. The keyed cursor rel
#     collapses 1 -> 3 across the batched tick while the log rel carries 2 and 3
#     as separate rows: state loses the intermediate, the stream does not.
two_door "(a) log rel + surface ordinal" ordinal_stream.dl6 ordinal_stream.schedule.json

# (b) The SAME program written as one left-to-right match block. Same log, so
#     the north-star spelling costs nothing.
two_door "(b) the same mint as one match block" match_stream.dl6 match_stream.schedule.json

# (c) keep(count(0)) is the table spelling of an rx Subject: the row fires every
#     edge rule that listens, appears in no tick log of its own, and is gone at
#     the boundary. Deliver-and-forget needs no construct.
two_door "(c) keep(count(0)) is deliver-and-forget" transient.dl6 transient.schedule.json

# (d) Eviction as an EVENT. A log rel's own tick log never reports what
#     retention removed (0_receipts.pl R12). One derived level rel over it does,
#     because the B plane has a minus, and finalize/1 over THAT fires. The
#     workaround for the log-finalize hole is one rule.
two_door "(d) eviction becomes an event one hop downstream" retention_event.dl6 retention_event.schedule.json

# (e) zip = equijoin on the ordinal; bufferCount = integer division on it.
#     `zip/2` is a RESERVED, refused word in registry.pl and needs nothing.
two_door "(e) zip and bufferCount are ordinary joins and arithmetic" zip_buffer.dl6 zip_buffer.schedule.json

# (f) latest/1 over a log rel samples the WHOLE table, not the last element.
#     The last element is max(ordinal) in a level rule. The rx word and the
#     stream word disagree; the design review's B8 vocabulary finding again.
two_door "(f) latest() over a log is not the last element" latest_log.dl6 latest_log.schedule.json

# (g) BACKPRESSURE with no construct. The writer is gated on the reader
#     watermark: two rows admitted, p3 and p4 refused into a `dropped` rel, the
#     reader advances, p5 admitted at ordinal 3. A bounded channel with N
#     readers therefore needs no retention rule at all, and the overflow is a
#     VISIBLE row where retention's own prune is silent
#     (0_receipts.pl R12, consumption-arms assertion 17).
two_door "(g) writer gated on the reader watermark, overflow visible" backpressure.dl6 backpressure.schedule.json

echo
echo "── leg 3: durability and sabotage ───────────────────────────────────────"

# (h) DURABILITY. rx has no durable subscription position; a table does. Two
#     server generations over one db file: the ordinal continues at 3 and the
#     first two rows are not re-delivered.
durability_receipt() {
  local scratch db gen1 gen2
  scratch="$(mktemp -d)"
  db="$scratch/durable.sqlite"
  gen1="$(served_final "$HERE/ordinal_stream.dl6" "file:$db" \
            '[{"rel":"event","sign":"add","row":["clicks","a"]}]' \
            '[{"rel":"event","sign":"add","row":["clicks","b"]}]')"
  gen2="$(served_final "$HERE/ordinal_stream.dl6" "file:$db" \
            '[{"rel":"event","sign":"add","row":["clicks","c"]}]')"
  rm -rf "$scratch"
  if [ "$gen1" = '{"rows":[["clicks",1,"a"],["clicks",2,"b"]]}' ] \
  && [ "$gen2" = '{"rows":[["clicks",1,"a"],["clicks",2,"b"],["clicks",3,"c"]]}' ]; then
    ok "(h) the ordinal survives a process restart and continues at 3"
  else
    bad "(h) durability: gen1=$gen1 gen2=$gen2"
  fi
}

served_final() {
  local program="$1" db="$2"; shift 2
  local scratch port pid batch loaded
  scratch="$(mktemp -d)"
  ( cd "$TSV2" && node --experimental-transform-types cli/bop.ts \
      serve --port 0 --db "$db" >"$scratch/serve.log" 2>&1 ) &
  pid=$!
  port=""
  for _ in $(seq 1 200); do
    port="$(sed -n 's/.*serving on \([0-9]*\).*/\1/p' "$scratch/serve.log" | head -1)"
    [ -n "$port" ] && break
    sleep 0.1
  done
  [ -n "$port" ] || { kill "$pid" 2>/dev/null; rm -rf "$scratch"; return 1; }
  local base="http://127.0.0.1:$port"
  loaded="$(curl -s -X POST --data-binary @"$program" "$base/program")"
  case "$loaded" in *'"loaded":true'*) : ;; *) kill "$pid" 2>/dev/null; rm -rf "$scratch"; return 1 ;; esac
  for batch in "$@"; do
    curl -s -X POST -d "{\"batch\":$batch}" "$base/arrivals" >/dev/null
    sleep 0.35
  done
  sleep 0.6
  curl -s "$base/idb/stream"
  kill "$pid" 2>/dev/null
  wait "$pid" 2>/dev/null
  rm -rf "$scratch"
}

durability_receipt

# (i) SABOTAGE. The grading in leg 2 discards tick numbers, so it has to be
#     shown that it still discriminates. One character changed in the ordinal
#     rule (At + 1 becomes At + 2) must make the two-door diff go red -- and it
#     must go red on the CONTENT of the stream, not on tick placement.
sabotage_receipt() {
  local scratch
  scratch="$(mktemp -d)"
  sed 's/Next := At + 1/Next := At + 2/' "$HERE/ordinal_stream.dl6" >"$scratch/sabotaged.dl6"
  oracle_log "$HERE/ordinal_stream.dl6" "$HERE/ordinal_stream.schedule.json" | sequence >"$scratch/good"
  oracle_log "$scratch/sabotaged.dl6"   "$HERE/ordinal_stream.schedule.json" | sequence >"$scratch/bad"
  if [ -s "$scratch/bad" ] && ! diff -q "$scratch/good" "$scratch/bad" >/dev/null; then
    ok "(i) sabotage: one changed increment makes the graded sequence differ"
  else
    bad "(i) sabotage did not register -- the comparison is not discriminating"
  fi
  rm -rf "$scratch"
}

sabotage_receipt

echo
printf '%s PASS %s FAIL\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
