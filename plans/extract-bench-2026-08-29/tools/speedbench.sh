#!/usr/bin/env bash
# speedbench.sh <lang> <out-prefix> [runs]
# Runs extract --resolve --family call,type over the corpus, 3 runs, wall+RSS.
set -euo pipefail
LAB="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$LAB"
BIN="$LAB/../../v6/sprefa-extract/target/release/extract"
lang="$1"; prefix="$2"; runs="${3:-3}"
case "$lang" in
  go) corpus=/Users/chrishafley/projects/typescript-go; find_args=(-name '*.go' -not -path '*/vendor/*') ;;
  ts) corpus=/Users/chrishafley/projects/TypeScript-5.9; find_args=(-name '*.ts' -path '*/src/*') ;;
  rust) corpus=/Users/chrishafley/projects/rust-analyzer; find_args=(-name '*.rs' -path '*/crates/*/src/*') ;;
  *) echo "unknown lang" >&2; exit 2 ;;
esac
mapfile -t files < <(cd "$corpus" && find . "${find_args[@]}" | sed "s|^\./|$corpus/|")
echo "${#files[@]} files"
mkdir -p "$ROOT/out"
for i in $(seq 1 "$runs"); do
  raw="$ROOT/out/$prefix.raw.$i.jsonl"
  tl="$ROOT/out/$prefix.time.$i.txt"
  /usr/bin/time -l timeout 120 "$BIN" --resolve --family call,type "${files[@]}" > "$raw" 2> "$tl"
  rss=$(grep -o '[0-9]* *maximum resident set size' "$tl" | awk '{print $1}')
  ms=$(grep -o '[0-9.]* *real' "$tl" | awk '{print int($1*1000)}')
  echo -e "$i\t$ms ms\t$((rss/1048576)) MB\t$(wc -l < "$raw") lines"
done
