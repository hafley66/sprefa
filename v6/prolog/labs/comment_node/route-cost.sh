#!/usr/bin/env bash
# route-cost.sh -- DELIVERABLE 1 receipt: the three text-acquisition routes
# priced on the SAME corpus (v6/prolog/*.pl, the dogfood target), with real
# runs, not estimates.
#
#   (a) extractor grows a `comment` family: span + line/col + kind + stripped
#       text, one JSONL line per comment. NOT BUILT (extractor-is-fixed);
#       priced by SIMULATING its exact output shape from the cst family plus a
#       byte-slice pass, and counting what the host would hand the engine.
#   (b) NO extractor change: cst comment spans (already emitted, already
#       string-literal-safe) + a text slice. Two sub-variants measured:
#       (b1) the whole cst stream reaches the host boundary and the program
#            filters kind == 'line_comment'  -- what a `sh` host declaring
#            (kind, start, end) actually does today;
#       (b2) the host template filters with grep before stdout, so only
#            comment lines cross the boundary.
#   (c) sh host does the whole job: `grep -n` over the file, no cst join.
#       String-literal safety is LOST; the false positive is witnessed by
#       string-safety.sh.
#
# The unit that matters is ROWS CROSSING THE BOUNDARY, because every crossed
# row is an EDB arrival the engine writes, diffs and refCounts. Wall time and
# stdout bytes are reported beside it.
set -uo pipefail
LAB="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$LAB/../../../.." && pwd)"
CORPUS_DIR="$ROOT/v6/prolog"
EX="${DL_EXTRACT_BIN:-$ROOT/v6/sprefa-extract/target/release/extract}"
OUT="${1:-$LAB/out}"
mkdir -p "$OUT"

[ -x "$EX" ] || { echo "FAIL no extract binary at $EX"; exit 1; }

FILES="$OUT/files.txt"
find "$CORPUS_DIR" -name '*.pl' | sort > "$FILES"
NFILES=$(wc -l < "$FILES" | tr -d ' ')
NBYTES=$(cat $(cat "$FILES") | wc -c | tr -d ' ')
echo "corpus: $NFILES prolog files, $NBYTES bytes"

# ── route b1: the whole cst stream crosses the boundary ─────────────────────
CST="$OUT/cst.jsonl"
: > "$CST"
T0=$(python3 -c 'import time;print(time.time())')
while read -r file; do
  nice -n 19 "$EX" --family cst "$file" 2>/dev/null >> "$CST"
done < "$FILES"
T1=$(python3 -c 'import time;print(time.time())')
B1_NODES=$(grep -c '"record":"node"' "$CST")
B1_ALL=$(wc -l < "$CST" | tr -d ' ')
B1_BYTES=$(wc -c < "$CST" | tr -d ' ')
B1_MS=$(python3 -c "print(round(($T1-$T0)*1000))")

# comment nodes only (what any of the routes ultimately wants)
COMMENTS=$(grep '"record":"node"' "$CST" | grep -c 'comment')
LINEC=$(grep -c '"kind":"line_comment"' "$CST")

# ── route b2: the host template filters before stdout ───────────────────────
CSTF="$OUT/cst-filtered.jsonl"
: > "$CSTF"
T0=$(python3 -c 'import time;print(time.time())')
while read -r file; do
  nice -n 19 "$EX" --family cst "$file" 2>/dev/null | grep '"kind":"line_comment"' >> "$CSTF"
done < "$FILES"
T1=$(python3 -c 'import time;print(time.time())')
B2_ROWS=$(wc -l < "$CSTF" | tr -d ' ')
B2_BYTES=$(wc -c < "$CSTF" | tr -d ' ')
B2_MS=$(python3 -c "print(round(($T1-$T0)*1000))")

# ── route c: grep -n, no cst ────────────────────────────────────────────────
GREPOUT="$OUT/grep.txt"
: > "$GREPOUT"
T0=$(python3 -c 'import time;print(time.time())')
while read -r file; do
  grep -n '%' "$file" >> "$GREPOUT"
done < "$FILES"
T1=$(python3 -c 'import time;print(time.time())')
C_ROWS=$(wc -l < "$GREPOUT" | tr -d ' ')
C_BYTES=$(wc -c < "$GREPOUT" | tr -d ' ')
C_MS=$(python3 -c "print(round(($T1-$T0)*1000))")

# ── route a: the simulated comment family (span + line/col + kind + text) ───
# One line per comment, produced from the cst spans by the same byte-slice a
# real `comment` family would do inside the extractor. This is the SHAPE
# price, not a claim the extractor change is free.
AOUT="$OUT/comment-family.jsonl"
python3 "$LAB/slice_comments.py" "$FILES" "$CST" > "$AOUT"
A_ROWS=$(wc -l < "$AOUT" | tr -d ' ')
A_BYTES=$(wc -c < "$AOUT" | tr -d ' ')

printf '\n%-6s %10s %12s %10s  %s\n' route rows bytes ms note
printf '%-6s %10s %12s %10s  %s\n' a "$A_ROWS" "$A_BYTES" "n/a" "simulated comment family (1 row/comment, text included)"
printf '%-6s %10s %12s %10s  %s\n' b1 "$B1_ALL" "$B1_BYTES" "$B1_MS" "whole cst stream crosses (nodes=$B1_NODES)"
printf '%-6s %10s %12s %10s  %s\n' b2 "$B2_ROWS" "$B2_BYTES" "$B2_MS" "host template pre-filters to line_comment"
printf '%-6s %10s %12s %10s  %s\n' c "$C_ROWS" "$C_BYTES" "$C_MS" "grep -n, no grammar (string-unsafe)"
printf '\ncomment nodes in cst: %s (line_comment %s)\n' "$COMMENTS" "$LINEC"
printf 'boundary amplification b1/a = %s\n' "$(python3 -c "print(round($B1_ALL/max($A_ROWS,1),1))")"
printf 'route c over-count vs a    = %s rows\n' "$((C_ROWS - A_ROWS))"
