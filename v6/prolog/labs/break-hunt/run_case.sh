#!/usr/bin/env bash
# run_case.sh NAME -- run one break-hunt case through BOTH doors and diff.
#
#   NAME.dl6            the program
#   NAME.schedule.json  the arrival schedule (sweep.pl's shape)
#
# Prints, in order: the compile leg, the ORACLE leg (reference engine over the
# .dl6 text), the EMITTED leg (prolog-emitted TS module on the tsv2 runtime),
# then the diff. Every leg is timed, because a leg over 10s is itself the
# finding.
#
# The emitted module and this lab's runner are copied into v6/tsv2/gen_emitted/
# (gitignored) so the module's own "../runtime/..." specifiers resolve.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../../.." && pwd)"
NAME="${1:?usage: run_case.sh NAME}"
GEN="$V6/tsv2/gen_emitted"
MOD="bh_$NAME"

mkdir -p "$GEN" "$HERE/out"

echo "=== COMPILE $NAME ==="
if ! timeout 60 swipl -q -l "$V6/prolog/compile.pl" \
      -g "compile_dl6('$HERE/$NAME.dl6', '$GEN/$MOD.ts')" -g halt \
      > "$HERE/out/$NAME.compile.txt" 2>&1; then
  echo "COMPILE_FAILED (exit $?)"
fi
cat "$HERE/out/$NAME.compile.txt"

echo "=== ORACLE $NAME ==="
timeout 60 swipl -q -l "$HERE/oracle_case.pl" \
  -g "oracle_case('$HERE/$NAME.dl6','$HERE/$NAME.schedule.json')" -g halt \
  > "$HERE/out/$NAME.oracle.txt" 2>&1
echo "oracle exit=$?"
cat "$HERE/out/$NAME.oracle.txt"

echo "=== EMITTED $NAME ==="
if [ -f "$GEN/$MOD.ts" ]; then
  cp "$HERE/run_case.ts" "$GEN/run_case.ts"
  ( cd "$V6/tsv2" && timeout 60 node --no-warnings --experimental-transform-types \
      gen_emitted/run_case.ts "$MOD" "$HERE/$NAME.schedule.json" ) \
    > "$HERE/out/$NAME.emitted.txt" 2>&1
  echo "emitted exit=$?"
  cat "$HERE/out/$NAME.emitted.txt"
  echo "=== DIFF (< oracle, > emitted) ==="
  diff "$HERE/out/$NAME.oracle.txt" "$HERE/out/$NAME.emitted.txt" && echo "IDENTICAL"
else
  echo "no emitted module; compile leg did not produce $MOD.ts"
fi
