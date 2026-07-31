#!/usr/bin/env bash
# run.sh <dir/name> [scheduleName] : authoring helper, one program through both
# doors. Not a receipt; receipts.sh is.
#
#   bash v6/prolog/labs/point_free/run.sh probe/scan_pre probe/scan_single
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
NAME="$1"
SCHED="${2:-$1}"
echo "-- oracle --"
( cd "$REPO/v6/prolog/compile/scripts" && swipl -q -l dl6_oracle.pl \
    -g "oracle('$HERE/$NAME.dl6','$HERE/$SCHED.schedule.json')" -g halt )
echo "-- bop check --"
( cd "$REPO/v6/tsv2" && npm run --silent bop -- check "$HERE/$NAME.dl6" 2>&1 ) \
  | grep -v 'ExperimentalWarning\|trace-warnings'
