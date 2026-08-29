#!/usr/bin/env bash
set -uo pipefail

LAB="$(cd "$(dirname "$0")" && pwd)"
BIN="$LAB/../../v6/sprefa-extract/target/release/extract"
OUT="${OUT_DIR:-$LAB/out}"
mkdir -p "$OUT"

run_one() {
  local lang="$1" root="$2" list="$3" mode="$4"
  local raw="$OUT/$lang.parse.$mode.single.raw.jsonl"
  local timelog="$OUT/$lang.$mode.single.time.txt"
  local runs="$OUT/$lang.$mode.single.runs.tsv"
  local argv=()
  case "$mode" in
    resolve)   argv=(--resolve --family call,type) ;;
    diet_scip) argv=(--family diet_scip) ;;
    deps)      argv=(--deps --project-root "$root") ;;
    *) echo "unknown mode $mode" >&2; return 2 ;;
  esac
  # go's list is the largest at ~400 KB of argv; ARG_MAX on this machine is 1048576.
  local files=()
  while IFS= read -r line; do [ -n "$line" ] && files+=("$root/$line"); done < "$list"
  local t0 t1
  t0=$(python3 -c 'import time;print(time.time())')
  /usr/bin/time -l timeout 60 "$BIN" "${argv[@]}" "${files[@]}" \
    > "$raw" 2> "$timelog"
  local rc=$?
  t1=$(python3 -c 'import time;print(time.time())')
  local ms
  ms=$(python3 -c "print(int(($t1-$t0)*1000))")
  local rss
  rss=$(grep -o '[0-9]* *maximum resident set size' "$timelog" | awk '{print $1}')
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$lang" "$mode" "${#files[@]}" "$rc" "$ms" "$(wc -l < "$raw" | tr -d ' ')" "${rss:-NA}" \
    > "$runs"
  cat "$runs"
}

run_one "$@"
