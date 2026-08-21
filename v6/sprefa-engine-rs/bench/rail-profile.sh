#!/usr/bin/env bash
# @comment-ok: the bench's usage contract, the one doc site for its arguments.
# rail-profile.sh <tree> <glob> [runs] [ENV=VALUE ...] -- run the dead-module
# rail's fold the way v6/dl/deadcode/dead-module-rail.sh runs it, but keeping
# stderr so the summary table survives, and timing the harness leg alone.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$HERE/.." && pwd)"
V6="$(cd "$ENGINE/.." && pwd)"
RAIL="$V6/dl/deadcode"
TARGET="$1"
GLOB="$2"
RUNS="${3:-3}"
shift 3 || shift 2
WORK="${TMPDIR:-/tmp}/rail-profile"
mkdir -p "$WORK"
if [ ! -f "$WORK/rail.rs" ] || [ "$RAIL/dead-module-rail.dl6" -nt "$WORK/rail.rs" ]; then
  swipl -q -l "$V6/prolog/compile.pl" -l "$V6/prolog/emit_rust.pl" \
    -g "compile_dl6('$RAIL/dead-module-rail.dl6','$WORK/rail.rs',[emitter(emit_rust:emit_program)])" \
    -g halt
fi
ROOTS="$( (cd "$TARGET" && cargo metadata --format-version 1 --no-deps 2>/dev/null) | python3 -c '
import json,os,sys
document = json.load(sys.stdin)
root = document["workspace_root"]
kinds = {"bin", "lib", "proc-macro", "cdylib", "rlib"}
seen = []
for package in document["packages"]:
    for target in package["targets"]:
        if set(target["kind"]) & kinds:
            path = os.path.relpath(target["src_path"], root)
            if path not in seen:
                seen.append(path)
print(",".join(sorted(seen)))
' )"
python3 -c "
import json,sys
seed=[{'rel':'want','sign':'add','row':[sys.argv[2]]}]
seed += [{'rel':'root_file','sign':'add','row':[r]} for r in sys.argv[3].split(',') if r]
json.dump([seed], open(sys.argv[1],'w'))
" "$WORK/schedule.json" "$GLOB" "$ROOTS"
now() { python3 -c 'import time;print(int(time.time()*1000))'; }
for index in $(seq 1 "$RUNS"); do
  START=$(now)
  ( cd "$TARGET" && env DL_ADAPTERS_DIR="$RAIL" \
      DL_EXTRACT_BIN="${DL_EXTRACT_BIN:-$V6/sprefa-extract/target/release/extract}" \
      DL_TRACE_SUMMARY=1 "$@" \
      "$ENGINE/target/release/emit_rust_harness" "$WORK/rail.rs" "$WORK/schedule.json" --live-hosts ) \
    >"$WORK/ticks.$index.jsonl" 2>"$WORK/summary.$index"
  printf 'rail run %s wall_ms=%s lines=%s\n' "$index" "$(($(now) - START))" \
    "$(wc -l <"$WORK/ticks.$index.jsonl" | tr -d ' ')"
done
cat "$WORK/summary.$RUNS"
