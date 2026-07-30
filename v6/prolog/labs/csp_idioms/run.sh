#!/usr/bin/env bash
# run.sh <name> : authoring helper. Runs one lab program through both doors.
#   bash run.sh workerpool          -> oracle tick log, then the compile check
# Not a receipt; receipts.sh is. This exists so the author can iterate.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
NAME="$1"
echo "── oracle ──"
( cd "$REPO/v6/prolog/compile/scripts" && swipl -q -l dl6_oracle.pl \
    -g "oracle('$HERE/$NAME.dl6','$HERE/$NAME.schedule.json')" -g halt )
echo "── bop check ──"
out="$( cd "$REPO/v6/tsv2" && npm run --silent bop -- check "$HERE/$NAME.dl6" 2>&1 )"
code=$?
printf '%s\n' "$out" | grep -v 'ExperimentalWarning\|trace-warnings'
echo "check exit: $code"
