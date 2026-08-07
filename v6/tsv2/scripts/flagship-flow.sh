#!/usr/bin/env bash
# flagship-flow.sh -- v5 flow-interproc versus the v6 value-plane port.
#
# The v6 rail reads df direct rows, positional df arg/param rows, resolved
# caller-site pins, and flat sig-owner fields. The classifier requires a
# nonzero flow_edge intersection: an all-gap table is a failed value-plane leg.
set -euo pipefail

TSV2="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$TSV2/../.." && pwd)"
PORT="${TSV2_FLAGSHIP_FLOW_PORT:-17574}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-flow.XXXXXX")"
ROOT="$WORK/root"
BASE="http://127.0.0.1:$PORT"
V5_PROGRAM="$REPO/examples/flow-interproc.dl"
V6_PROGRAM="$TSV2/../dl/fixtures/flagship-flow.dl6"
SERVE_MAIN="$TSV2/serve/main.ts"
CLASSIFY="$TSV2/scripts/flagship-flow-classify.py"
SERVER_PID=""

CORPUS="
src/types.rs
src/wire.rs
src/scip.rs
src/dispatch.rs
src/lib.rs
src/family.rs
src/rows.rs
src/seams.rs
src/shape.rs
src/source.rs
src/lang/mod.rs
src/lang/astgrep.rs
src/bin/extract.rs
"

fail() { printf 'FAIL  %s\n' "$*"; [ -n "$SERVER_PID" ] && tail -c 2000 "$WORK/server.log"; stop_server; exit 1; }
say() { printf '%s\n' "$*"; }
stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null || true
  [ -n "$SERVER_PID" ] && wait "$SERVER_PID" 2>/dev/null || true
  SERVER_PID=""
}
trap stop_server EXIT

resolve_v5_bin() {
  if [ -n "${DL_V5_BIN:-}" ] && [ -x "$DL_V5_BIN" ]; then return; fi
  DL_V5_BIN="$REPO/target/release/dl"
  [ -x "$DL_V5_BIN" ] || fail "no v5 release binary at $DL_V5_BIN; build cargo --release --bin dl first"
}

resolve_extract_bin() {
  if [ -n "${DL_EXTRACT_BIN:-}" ] && [ -x "$DL_EXTRACT_BIN" ]; then return; fi
  DL_EXTRACT_BIN="$REPO/v6/sprefa-extract/target/release/extract"
  [ -x "$DL_EXTRACT_BIN" ] || fail "no v6 release extractor at $DL_EXTRACT_BIN; build cargo --release --features cli --bin extract first"
  export DL_EXTRACT_BIN
}

build_corpus() {
  local count=0 file
  for file in $CORPUS; do
    [ -f "$REPO/v6/sprefa-extract/$file" ] || fail "pinned corpus file missing: $file"
    mkdir -p "$ROOT/$(dirname "$file")"
    cp -f "$REPO/v6/sprefa-extract/$file" "$ROOT/$file"
    count=$((count + 1))
  done
  (cd "$ROOT" && git init -q . && git add -A && git -c user.email=rig@sprefa -c user.name=flagship-rig commit -q -m pinned-corpus) || fail "could not initialize corpus"
  REV="$(git -C "$ROOT" rev-parse HEAD)"
  CORPUS_N="$count"
  printf '%s\n' $CORPUS | LC_ALL=C sort >"$WORK/fileset.pin"
  say "PASS  pinned corpus: $CORPUS_N files at $REV"
}

rows_of() { curl -s "$BASE/idb/$1"; }
row_count() { rows_of "$1" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["rows"]))'; }
dump_v6() { rows_of "$1" | python3 -c 'import json,sys; rows=json.load(sys.stdin)["rows"]; print("\n".join(sorted({"\t".join(str(value) for value in row) for row in rows})))' >"$2"; }

resolve_v5_bin
resolve_extract_bin
build_corpus

cd "$ROOT"
SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1 DL_STATE_DIR="$WORK/state" \
  "$DL_V5_BIN" "$V5_PROGRAM" --db "$WORK/v5.sqlite" >"$WORK/v5.out" 2>"$WORK/v5.err" \
  || fail "v5 run failed: $(tail -10 "$WORK/v5.err")"
[ -f "$WORK/state/invocations.db" ] || fail "v5 state escaped DL_STATE_DIR"
v5_files="$(sqlite3 "$WORK/v5.sqlite" 'select count(*) from rel_src;')"
[ "$v5_files" = "$CORPUS_N" ] || fail "v5 saw $v5_files files, pin has $CORPUS_N"
say "PASS  v5 hermetic leg: $v5_files files"

for rel in flow_edge flow_reach flow_param_type flow_node_type; do
  sqlite3 -noheader -separator '	' "$WORK/v5.sqlite" "select distinct * from rel_${rel}_txt;" | LC_ALL=C sort -u >"$WORK/v5.$rel.tsv"
done

TSV2_DB=":memory:" TSV2_PORT="$PORT" DL_EXTRACT_BIN="$DL_EXTRACT_BIN" \
  node --experimental-transform-types "$SERVE_MAIN" >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 60); do
  curl -s -o /dev/null "$BASE/ticks" 2>/dev/null && break
  kill -0 "$SERVER_PID" 2>/dev/null || fail "server died on boot: $(tail -5 "$WORK/server.log")"
  sleep 0.2
done

status="$(curl -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$V6_PROGRAM" "$BASE/program")"
[ "$status" = 200 ] || fail "v6 program load returned $status: $(cat "$WORK/load.json")"
curl -s -o /dev/null -X POST --data-binary "{\"batch\":[{\"rel\":\"want_at\",\"sign\":\"add\",\"row\":[\"$REV\",\"src/*.rs\"]}]}" "$BASE/edb/events"

settled=""; last=""; stable=0; deadline=$((SECONDS + 300))
while [ "$SECONDS" -lt "$deadline" ]; do
  now="$(row_count flow_edge):$(row_count flow_reach):$(row_count flow_param_type):$(row_count flow_node_type)"
  if [ "$now" = "$last" ]; then
    stable=$((stable + 1))
    [ "$stable" -ge 3 ] && { settled="$now"; break; }
  else
    stable=0
  fi
  last="$now"
  sleep 1
done
[ -n "$settled" ] || fail "v6 never settled (last flow_edge:flow_reach:flow_param_type:flow_node_type = $last)"
say "PASS  v6 settled: flow_edge:flow_reach:flow_param_type:flow_node_type = $settled"

for rel in flow_edge flow_reach flow_param_type flow_node_type; do
  dump_v6 "$rel" "$WORK/v6.$rel.tsv"
done
for rel in df_direct flow_arg_edge flow_ret_edge call_target; do
  dump_v6 "$rel" "$WORK/v6.$rel.tsv"
done

python3 "$CLASSIFY" "$WORK"
say "artifacts: $WORK"
say "FLAGSHIP FLOW GRADED: value-plane counts, match assertions, 0 unclassified"
