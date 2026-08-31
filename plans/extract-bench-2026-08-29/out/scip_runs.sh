#!/usr/bin/env bash
# The SCIP receipt driver: one process per corpus per arm, then the normal
# forms and the bench comparisons against the bare vta oracle for go.
# Walls: informed ts ~3.5s rust ~5s go ~24-33s after the scip.rs caches.
set -uo pipefail
LAB="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$LAB/../../v6/sprefa-extract/target/release/extract"
OUT="$LAB/out"
ROOT=/Users/chrishafley/projects

run() {
  local lang="$1" root="$2" proot="$3" idx="$4" mode="$5" tmo="$6"; shift 6
  local files=()
  while IFS= read -r line; do [ -n "$line" ] && files+=("$root/$line"); done < "$OUT/$lang.files.txt"
  local t0 t1 rc
  t0=$(python3 -c 'import time;print(time.time())')
  if [ "$mode" = rawscip ]; then
    timeout "$tmo" "$BIN" --family scip --project-root "$proot" "$proot" > "$OUT/$lang.$mode.raw.jsonl" 2> "$OUT/$lang.$mode.err.txt"
  else
    timeout "$tmo" "$BIN" "$@" --resolve --family call,type --project-root "$proot" --scip-index "$idx" "${files[@]}" > "$OUT/$lang.$mode.raw.jsonl" 2> "$OUT/$lang.$mode.err.txt"
  fi
  rc=$?
  t1=$(python3 -c 'import time;print(time.time())')
  python3 -c "print(f'$lang $mode rc=$rc wall={($t1-$t0):.1f}s lines=' + str(sum(1 for _ in open('$OUT/$lang.$mode.raw.jsonl'))))"
}

bench() {
  local lang="$1" arm="$2" oracle="$3"
  (cd "$LAB" && python3 normalize.py resolved "out/$lang.$arm.raw.jsonl" "$OUT_ROOTS/$lang" "out/$lang.$arm.call.tsv" "out/$lang.$arm.type.tsv")
  (cd "$LAB" && python3 bench.py "out/$lang.$arm.call.tsv" "$oracle" | sed -n '3,7p')
}

# corpus roots for normalize.py: SCIP document paths are relative to these.
declare -A OUT_ROOTS=( [go]="$ROOT/typescript-go" [ts]="$ROOT/TypeScript-5.9/src" [rust]="$ROOT/rust-analyzer" )

# go: the oracle is the BARE vta table (ORACLES.REPORT.md 12); the receiver-
# prefixed go.oracle.call.vta.tsv is what produced the bogus 9.1% rows.
bench go plain    go.oracle.call.vta.bare.tsv
bench go scipinformed go.oracle.call.vta.bare.tsv
(cd "$LAB" && python3 normalize.py scip_call out/go.rawscip.raw.jsonl "$ROOT/typescript-go" out/go.rawscip.call.tsv && python3 bench.py out/go.rawscip.call.tsv go.oracle.call.vta.bare.tsv | sed -n '3,7p')
