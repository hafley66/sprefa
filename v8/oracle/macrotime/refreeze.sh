#!/usr/bin/env bash
# Refreeze every macrotime oracle case from v7. Run from a sprefa tree that
# holds a v7; V7_DIR selects the v7 the cases are measured against.
#
#   V7_DIR=$PWD/v7 bash v8/oracle/macrotime/refreeze.sh
set -eu
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out="${OUT:-$here}"
v7="${V7_DIR:-v7}"
rm -f "$out"/*_[0-9].json
for path in "$v7"/test/fixtures/*.dl7 "$here"/expansion_cases.dl7; do
    stem="$(basename "$path" .dl7)"
    rc=0
    V7_DIR="$v7" timeout 600 swipl "$here/dump_macrotime.pl" -- \
        "$path" "$out/$stem" >/dev/null 2>&1 || rc=$?
    echo "$stem rc=$rc"
done
# No .dl7 that compiles reaches a macrotime diagnostic, so those cases mutate
# one real unit instead.
V7_DIR="$v7" timeout 600 swipl "$here/dump_diagnostics.pl" -- \
    "$here/expansion_cases.dl7" "$out" >/dev/null 2>&1
# The prelude and macrotime units reify identically for every fixture; one copy
# of each distinct case is enough.
python3 "$here/dedup.py" "$out"
