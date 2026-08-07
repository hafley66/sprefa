#!/usr/bin/env bash
# scale.sh : the Q4 price table.
#
#   bash v6/prolog/labs/extract_t2/scale.sh
#
# Per schema document: input bytes, facts derived, reference-engine wall, and
# served-engine wall. The served number is the wall of the ONE `POST /edb/events`
# that carries the document -- that request returns only after the tick has
# settled and its log is written, so it is the tick, plus one loopback HTTP
# round trip (measured separately below as the floor).
#
# Also emitted: the STATEMENT COUNT of each compiled program, which is the
# count-test-law number here. It is a property of the PROGRAM, not of the
# document, so it must not move when the document grows -- struct.proto's
# descriptor (2 KB) and descriptor.proto's (45 KB) run the same program, so the
# same count must serve both, and the table prints it once per program to make
# that checkable.

set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
TSV2="$REPO/v6/tsv2"
export SPREFA_CONFIG=/nonexistent/extract-t2.toml
export DL_NO_DAEMON=1

SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

millis() { python3 -c 'import time; print(int(time.time()*1000))'; }

facts_and_oracle_ms() {
  # $1 program, $2 schedule -> "<facts> <ms>"
  local program="$1" schedule="$2" start finish
  start="$(millis)"
  ( cd "$HERE" && swipl -q -l t2_oracle.pl -g "oracle('$program','$schedule')" -g halt ) \
    >"$SCRATCH/oracle.jsonl" 2>/dev/null
  finish="$(millis)"
  python3 - "$SCRATCH/oracle.jsonl" "$((finish - start))" <<'PY'
import json, sys
facts = 0
for raw in open(sys.argv[1], encoding="utf-8"):
    raw = raw.strip()
    if not raw:
        continue
    for rel, delta in json.loads(raw)["deltas"].items():
        facts += len(delta.get("add", []))
print(facts, sys.argv[2])
PY
}

served_ms() {
  # $1 program, $2 schedule -> "<arrival_ms> <empty_post_floor_ms>"
  local program="$1" schedule="$2" scratch port pid batch start finish floor_start floor_finish
  scratch="$(mktemp -d)"
  ( cd "$TSV2" && node --experimental-transform-types cli/bop.ts \
      serve --port 0 --db ":memory:" >"$scratch/serve.log" 2>&1 ) &
  pid=$!
  port=""
  for _ in $(seq 1 300); do
    port="$(sed -n 's/.*serving on \([0-9]*\).*/\1/p' "$scratch/serve.log" | head -1)"
    [ -n "$port" ] && break
    sleep 0.1
  done
  local base="http://127.0.0.1:$port"
  curl -s -X POST --data-binary @"$program" "$base/program" >/dev/null
  # the floor: an arrivals POST carrying nothing, so HTTP + drain with no work
  floor_start="$(millis)"
  curl -s -X POST -d '{"batch":[]}' "$base/edb/events" >/dev/null
  floor_finish="$(millis)"
  batch="$(python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))[0]))' "$schedule")"
  start="$(millis)"
  curl -s -X POST -d "{\"batch\":$batch}" "$base/edb/events" >/dev/null
  finish="$(millis)"
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
  rm -rf "$scratch"
  echo "$((finish - start)) $((floor_finish - floor_start))"
}

statement_count() {
  # $1 program -> number of SQL statements in the compiled module
  local program="$1"
  ( cd "$HERE" && bash "$REPO/v6/prolog/compile/scripts/compile_dl6.sh" \
      "$program" "$SCRATCH/compiled.ts" ) >/dev/null 2>&1 || { echo "n/a"; return; }
  grep -c 'sql: `' "$SCRATCH/compiled.ts" 2>/dev/null || echo "n/a"
}

row() {
  local label="$1" program="$2" schedule="$3" source="$4"
  local bytes facts oracle served floor stmts
  bytes="$(wc -c <"$HERE/$source" | tr -d ' ')"
  read -r facts oracle <<<"$(facts_and_oracle_ms "$program" "$schedule")"
  read -r served floor <<<"$(served_ms "$HERE/$program" "$HERE/$schedule")"
  stmts="$(statement_count "$program")"
  printf '%-22s %9s %7s %10s %10s %9s %8s\n' \
    "$label" "$bytes" "$facts" "$oracle" "$served" "$floor" "$stmts"
}

printf '%-22s %9s %7s %10s %10s %9s %8s\n' \
  document bytes facts oracle_ms served_ms floor_ms stmts
printf '%-22s %9s %7s %10s %10s %9s %8s\n' \
  ---------------------- --------- ------- ---------- ---------- --------- --------
row "openapi petstore"  openapi.dl6 openapi.schedule.json    corpus/openapi-petstore.json
row "avro interop"      avro.dl6    avro.schedule.json       corpus/avro-interop.avsc
row "proto struct"      proto.dl6   proto.schedule.json      corpus/proto-struct.json
row "proto descriptor"  proto.dl6   proto-big.schedule.json  corpus/proto-descriptor.json
row "graphql swapi"     graphql.dl6 graphql.schedule.json    corpus/graphql-swapi-introspection.json
row "xrepo federation"  xrepo.dl6   xrepo.schedule.json      xrepo.schedule.json
