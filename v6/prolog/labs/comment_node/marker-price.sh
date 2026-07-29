#!/usr/bin/env bash
# marker-price.sh -- DELIVERABLE 2: the three marker-capture routes priced on
# the same corpus, so SLOT-MARKER-CAPTURE is a number and not a preference.
#
#   (a) per-marker sh host. One host declaration per convention; the regex and
#       any capture-group surgery live in the TEMPLATE. This is what both
#       receipt programs do. Cost measured: one subprocess per (file,
#       convention), so the seven golden techniques over N files is 7N spawns
#       where the cst comment host alone is N.
#   (b) extractor-side marker splitting. Not measured because it is refused on
#       principle before it is refused on cost: `std/suppress.dl`'s own header
#       states the law -- "policy lives HERE, never in Rust" -- and `ARCH`,
#       `dl-disable-line`, `TODO`, `LANG-JUNCTION`, `BEGIN:` and `README(` are
#       six conventions, five of which exist only in this repository's own
#       example corpus. Baking any of them into a fixed extractor makes the
#       extractor the place a convention is added.
#   (c) a text/regex construct in the language. Priced by counting what it
#       would have to replace (gap-inventory.sh) and by naming the program
#       that would PROVE it necessary. See the verdict: the seven techniques
#       do NOT prove it, because every text operation they perform runs on
#       bytes from ONE file and is therefore pushable into that file's host.
set -uo pipefail
LAB="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$LAB/../../../.." && pwd)"
EX="${DL_EXTRACT_BIN:-$ROOT/v6/sprefa-extract/target/release/extract}"
OUT="${1:-$LAB/out}"
mkdir -p "$OUT"

FILES="$OUT/files.txt"
find "$ROOT/v6/prolog" -name '*.pl' -not -path '*/labs/*' | sort > "$FILES"
NFILES=$(wc -l < "$FILES" | tr -d ' ')

# The six marker conventions the seven golden techniques use, as the grep each
# one's host template would run. Verbatim from the v5 rails.
CONVENTIONS='ARCH\ *\{|dl-(disable|enable)|LANG-JUNCTION\(|README\(|todo\(|TODO|FIXME|BEGIN:'

time_ms() {
  local start end
  start=$(python3 -c 'import time;print(time.time())')
  "$@" >/dev/null 2>&1
  end=$(python3 -c 'import time;print(time.time())')
  python3 -c "print(round(($end-$start)*1000))"
}

one_grep_per_file() {
  while read -r file; do grep -nE "$CONVENTIONS" "$file"; done < "$FILES"
}

seven_greps_per_file() {
  while read -r file; do
    for pattern in 'ARCH *\{' 'dl-(disable|enable)' 'LANG-JUNCTION\(' 'README\(' 'todo\(' 'TODO|FIXME' 'BEGIN:'; do
      grep -nE "$pattern" "$file"
    done
  done < "$FILES"
}

one_extract_per_file() {
  while read -r file; do nice -n 19 "$EX" --family cst "$file"; done < "$FILES"
}

ONE=$(time_ms one_grep_per_file)
SEVEN=$(time_ms seven_greps_per_file)
CST=$(time_ms one_extract_per_file)

echo "corpus: $NFILES files (v6/prolog, the dogfood target)"
printf '%-42s %6s ms   %s spawns\n' 'one marker host (all conventions, 1 grep)' "$ONE"   "$NFILES"
printf '%-42s %6s ms   %s spawns\n' 'seven marker hosts (route a, as declared)' "$SEVEN" "$((NFILES * 7))"
printf '%-42s %6s ms   %s spawns\n' 'the cst comment host (the grammar witness)' "$CST"  "$NFILES"
echo
printf 'route (a) marker cost over the grammar host: %sx wall, %sx spawns\n' \
  "$(python3 -c "print(round($SEVEN/max($CST,1),2))")" 7
printf 'collapsing seven conventions into one host: %sx wall saved\n' \
  "$(python3 -c "print(round($SEVEN/max($ONE,1),1))")"
