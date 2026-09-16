#!/usr/bin/env bash
# 4_receipts.sh : run the alloy_semantics pipeline and capture the four
# receipts: green run, sabotage 1 (unresolved_ref), sabotage 2
# (duplicate_name), and ts parity against the real spine.ts / types.ts.
#
#   bash 4_receipts.sh
set -u

cd "$(dirname "$0")"

say() { echo; echo "\$ $1"; }

echo "############################################################"
echo "# (a) GREEN RUN - both targets, import/use lines derived from ref/2"
echo "############################################################"
say "swipl -q -l run.pl -g run -g halt"
swipl -q -l run.pl -g run -g halt

echo
echo "############################################################"
echo "# (d1) capture emitted ts (used by (d) parity below)"
echo "############################################################"
say "swipl -q -l run.pl -g \"ts_text(T),format('~s',[T])\" -g halt"
swipl -q -l run.pl -g "ts_text(T),format('~s',[T])" -g halt > ts_out.txt 2>ts_err.txt

echo
echo "############################################################"
echo "# (b) SABOTAGE 1 - comment out the node decl (env hook)"
echo "#     expect codegen_refused(unresolved_ref) and NO text emitted"
echo "############################################################"
say "ALLOW_LAB_SABOTAGE_UNRESOLVED=1 swipl -q -l run.pl -g run -g halt"
ALLOW_LAB_SABOTAGE_UNRESOLVED=1 swipl -q -l run.pl -g run -g halt
echo "exit=$?"

echo
echo "############################################################"
echo "# (c) SABOTAGE 2 - add a duplicate rendered decl name (env hook)"
echo "#     expect codegen_refused(duplicate_name) and NO text emitted"
echo "############################################################"
say "ALLOW_LAB_SABOTAGE_DUPLICATE=1 swipl -q -l run.pl -g run -g halt"
ALLOW_LAB_SABOTAGE_DUPLICATE=1 swipl -q -l run.pl -g run -g halt
echo "exit=$?"

echo
echo "############################################################"
echo "# (d) PARITY - emitted ts field names+types vs the real spine"
echo "#     strings: spine.ts:63-66 ; node/edge: types.ts:80-95"
echo "#     exact match not required; field names+types must match"
echo "############################################################"
echo "--- emitted ts ---"
cat ts_out.txt

grep -oE '^  [a-z_]+: [^;]+;' ts_out.txt | sort > par_emit.txt
{
  sed -n '63,66p' ../../../sprefa-store/js/src/engine/spine.ts
  sed -n '80,95p' ../../../sprefa-store/js/src/engine/types.ts
} | grep -oE '^  [a-z_]+: [^;]+;' | sort > par_real.txt

diff par_real.txt par_emit.txt > par.diff
echo
echo "diff (real vs emitted) exit=$?   (0 = field-for-field identical)"
cat par.diff
echo
echo "--- reference field lines (real checked-in interfaces) ---"
cat par_real.txt
echo

rm -f par_emit.txt par_real.txt par.diff
