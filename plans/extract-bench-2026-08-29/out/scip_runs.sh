#!/usr/bin/env bash
set -uo pipefail
LAB="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$LAB/../../v6/sprefa-extract/target/release/extract"
OUT="$LAB/out"

run() {
  local lang="$1" root="$2" idx="$3" mode="$4"; shift 4
  local files=()
  while IFS= read -r line; do [ -n "$line" ] && files+=("$root/$line"); done < "$OUT/$lang.files.txt"
  local t0 t1 rc
  t0=$(python3 -c 'import time;print(time.time())')
  timeout 60 "$BIN" "$@" "${files[@]}" > "$OUT/$lang.$mode.raw.jsonl" 2> "$OUT/$lang.$mode.err.txt"
  rc=$?
  t1=$(python3 -c 'import time;print(time.time())')
  python3 -c "print(f'$lang $mode rc=$rc wall={($t1-$t0):.1f}s lines=' + str(sum(1 for _ in open('$OUT/$lang.$mode.raw.jsonl'))))"
}

for lang_root in \
  "go /Users/chrishafley/projects/typescript-go /Users/chrishafley/projects/typescript-go/index.scip" \
  "ts /Users/chrishafley/projects/TypeScript-5.9 /Users/chrishafley/projects/TypeScript-5.9/src/.dl/.state/index.scip" \
  "rust /Users/chrishafley/projects/rust-analyzer /Users/chrishafley/projects/rust-analyzer/.dl/.state/index.scip"; do
  read -r lang root idx <<< "$lang_root"
  run "$lang" "$root" "$idx" plain --resolve --family call,type
  run "$lang" "$root" "$idx" scipinformed --resolve --family call,type --project-root "$root" --scip-index "$idx"
done