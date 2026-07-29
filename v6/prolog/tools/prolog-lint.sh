#!/usr/bin/env bash
# prolog-lint.sh -- read-only organization gate over v6/prolog/**/*.pl.
#
# Runs the three phases of tools/prolog_lint.pl in separate SWI processes (the
# tree cannot be loaded all at once), collects their FINDING lines, and
# ratchets them against tools/prolog-lint-baseline.txt.
#
# Exit codes:
#   0  findings exactly match the baseline
#   1  a NEW finding appeared, or a baseline entry was fixed without pruning
#      the baseline file (the ratchet only moves down, and only on purpose)
#
# Findings that fail the gate: parse errors, duplicate module names, undefined
# predicates, private cross-module calls, redefinitions, void declarations,
# trivial fails, format errors. Unused exports are advisory output only,
# because `-g` entry points, ensure_loaded/1 reach, and meta-calls are
# indistinguishable from dead code at the source level.
#
# Refresh the baseline after deliberately changing findings:
#   bash tools/prolog-lint.sh --write-baseline

set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
prolog_root="$(cd "$here/.." && pwd)"
baseline="$here/prolog-lint-baseline.txt"

write_baseline=0
if [ "${1:-}" = "--write-baseline" ]; then
  write_baseline=1
fi

raw="$(mktemp)"
findings="$(mktemp)"
advisories="$(mktemp)"
trap 'rm -f "$raw" "$findings" "$advisories"' EXIT

run_phase() {
  local goal="$1"
  ( cd "$prolog_root" && swipl -q -l tools/prolog_lint.pl -g "$goal" -g halt 2>/dev/null )
}

{
  run_phase 'lint_sources'
  run_phase 'lint_loaded(compile)'
  run_phase 'lint_loaded(example)'
} > "$raw"

grep '^FINDING' "$raw" | LC_ALL=C sort > "$findings" || true
grep '^ADVISORY' "$raw" | LC_ALL=C sort > "$advisories" || true

if [ "$write_baseline" = "1" ]; then
  cp "$findings" "$baseline"
  echo "prolog-lint: baseline written, $(wc -l < "$baseline" | tr -d ' ') entries"
  exit 0
fi

if [ ! -f "$baseline" ]; then
  echo "prolog-lint: no baseline at $baseline" >&2
  exit 1
fi

sorted_baseline="$(mktemp)"
trap 'rm -f "$raw" "$findings" "$advisories" "$sorted_baseline"' EXIT
LC_ALL=C sort "$baseline" > "$sorted_baseline"

new_findings="$(comm -23 "$findings" "$sorted_baseline")"
fixed_findings="$(comm -13 "$findings" "$sorted_baseline")"

echo "── advisory: unused-export candidates and counts ──"
sed 's/^ADVISORY\t//' "$advisories"
echo

status=0

if [ -n "$new_findings" ]; then
  echo "── NEW findings (gate failure) ──"
  echo "$new_findings"
  echo
  status=1
fi

if [ -n "$fixed_findings" ]; then
  echo "── baseline entries no longer present (prune the baseline) ──"
  echo "$fixed_findings"
  echo "Run: bash tools/prolog-lint.sh --write-baseline"
  echo
  status=1
fi

total="$(wc -l < "$findings" | tr -d ' ')"
expected="$(wc -l < "$sorted_baseline" | tr -d ' ')"
if [ "$status" = "0" ]; then
  echo "PROLOG_LINT findings=$total baseline=$expected OK"
else
  echo "PROLOG_LINT findings=$total baseline=$expected FAIL"
fi
exit "$status"
