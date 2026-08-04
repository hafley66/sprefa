#!/usr/bin/env bash
# The README's fail-first receipt, executable: RED on a 3-line comment run,
# GREEN after trimming it to 2, and GREEN on a 1-prose block (delimiters are
# glue, not prose). Any other verdicts fail this script.
set -uo pipefail

GOLDEN_DIR="$(cd "$(dirname "$0")" && pwd)"
TSV2_DIR="$(cd "$GOLDEN_DIR/../.." && pwd)"
RAIL="$TSV2_DIR/scripts/comment-budget-rail.sh"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/comment-rail-failfirst.XXXXXX")"
trap 'rm -rf -- "$WORK"' EXIT

mkdir -p "$WORK/repo/src"
git -C "$WORK/repo" init -q
git -C "$WORK/repo" config user.email failfirst@example.com
git -C "$WORK/repo" config user.name failfirst
git -C "$WORK/repo" commit -q --allow-empty -m base

printf '%s\n' 'export const value = 1;' '// the engine cannot show this' \
  '// nor this' '// nor this third line' 'export const other = 2;' \
  >"$WORK/repo/src/subject.ts"
git -C "$WORK/repo" add src/subject.ts
( cd "$WORK/repo" && bash "$RAIL" ) >"$WORK/red.out" 2>"$WORK/red.err"
RED_STATUS=$?

printf '%s\n' 'export const value = 1;' '// the engine cannot show this' \
  '// nor this' 'export const other = 2;' >"$WORK/repo/src/subject.ts"
git -C "$WORK/repo" add src/subject.ts
( cd "$WORK/repo" && bash "$RAIL" ) >"$WORK/green.out" 2>"$WORK/green.err"
GREEN_STATUS=$?

# BLOCK leg: a `/* ... */` block with ONE prose line and the two delimiters
# wraps it. The delimiters are glue that keeps the run contiguous but add
# nothing to the measure, so the block is clean (prose count 1), not a 3-line
# violation.
printf '%s\n' '/*' 'one prose line' '*/' 'export const value = 1;' \
  >"$WORK/repo/src/block.ts"
git -C "$WORK/repo" add src/block.ts
( cd "$WORK/repo" && bash "$RAIL" ) >"$WORK/block.out" 2>"$WORK/block.err"
BLOCK_STATUS=$?

echo "── RED leg (3 comment lines), exit $RED_STATUS ──"
cat "$WORK/red.err"
echo "── GREEN leg (2 comment lines), exit $GREEN_STATUS ──"
cat "$WORK/green.err"
echo "── BLOCK leg (1 prose line in a 3-line block), exit $BLOCK_STATUS ──"
cat "$WORK/block.err"

if [ "$RED_STATUS" != 2 ]; then
  echo "FAIL  fail-first: 3-line run exited $RED_STATUS, want 2" >&2
  exit 1
fi
if ! grep -q 'src/subject.ts:2-4 (3 comment lines)' "$WORK/red.err"; then
  echo "FAIL  fail-first: red leg did not print src/subject.ts:2-4 (3 comment lines)" >&2
  exit 1
fi
if [ "$GREEN_STATUS" != 0 ]; then
  echo "FAIL  fail-first: 2-line run exited $GREEN_STATUS, want 0" >&2
  exit 1
fi
if [ "$BLOCK_STATUS" != 0 ]; then
  echo "FAIL  fail-first: 1-prose block exited $BLOCK_STATUS, want 0" >&2
  exit 1
fi
echo 'COMMENT_RAIL_FAIL_FIRST HOLDS red=2 green=0 block=0'
