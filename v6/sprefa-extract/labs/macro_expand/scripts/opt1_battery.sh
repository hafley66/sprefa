#!/bin/bash
# Option 1 corpus battery: RA crates src bucket, per file:
# expand to fixpoint (in-process mbe), extract call family on original vs
# expanded, count gained sites. timeout 10 per invocation.
LAB="$(pwd)/target/release/macro_expand_lab"
EX=/Users/chrishafley/projects/sprefa/.boop-worktrees/lab/extract-rust-macros/v6/sprefa-extract/target/release/extract
RA=/Users/chrishafley/projects/rust-analyzer
OUT="$(pwd)/out_opt1"
LOG="$(pwd)/opt1.battery.log"
mkdir -p "$OUT"
rm -f "$LOG"

run_one() {
  f="$RA/$1"
  h=$(printf '%s' "$1" | shasum | cut -c1-10)
  d="$OUT/$h"
  mkdir -p "$d"
  cd "$d" || return
  row=$(LAB_OUT_DIR="$d" timeout 10 "$LAB" fixture "$f" 2>/dev/null | tail -1)
  inv=$(echo "$row" | grep -o 'invocations=[0-9]*' | head -1 | cut -d= -f2)
  if [ -z "$inv" ]; then
    echo -e "$1\tSKIP\tms=NA\t0\t0\t0" >> "$LOG"
    cd /; rm -rf "$d"; return
  fi
  ms=$(echo "$row" | grep -o 'ms=[0-9]*' | cut -d= -f2 | awk '{s+=$1} END {print s}')
  expfile=$(ls "$d"/*.expanded.rs* 2>/dev/null | tail -1)
  [ "$expfile" = "$f.expanded.rs" ] && expfile=""
  o_sites=$(timeout 10 "$EX" --family call "$f" 2>/dev/null | grep -c '"record":"site"')
  if [ -f "$expfile" ]; then
    e_sites=$(timeout 10 "$EX" --family call "$expfile" 2>/dev/null | grep -c '"record":"site"')
  else
    e_sites=$o_sites
  fi
  gained=$((e_sites - o_sites))
  echo -e "$1\tinv=$inv\tms=$ms\t$o_sites\t$e_sites\t$gained" >> "$LOG"
  cd /; rm -rf "$d"
}
export -f run_one
export LAB EX RA OUT LOG

cd "$(dirname "$0")/.." || exit 1
cd "$(pwd)" >/dev/null
cd - >/dev/null 2>&1
cat /tmp/ra_src_files.txt | xargs -P 8 -n 1 -I{} bash -c 'run_one "$@"' _ {}
echo DONE >> "$LOG"
