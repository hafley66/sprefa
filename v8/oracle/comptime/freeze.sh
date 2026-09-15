#!/usr/bin/env bash
set -u
# bash v8/oracle/comptime/freeze.sh [sprefa-repo] [rev]
# REV is pinned, never HEAD: a v8 branch can carry an older v7 than main.
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=${1:-$(cd "$HERE/../../.." && pwd)}
REV=${2:-f5018ad23}
STAGE=/tmp/dl8-comptime-oracle
SCRATCH=$STAGE/out
rm -rf "$STAGE"
mkdir -p "$SCRATCH"
git -C "$REPO" archive "$REV" v7 | tar -x -C "$STAGE"
export V7_DIR=$STAGE/v7

for fixture in "$STAGE"/v7/test/fixtures/*.dl7 "$STAGE"/v7/applications/dl6/*.dl7; do
  stem=$(basename "$fixture" .dl7)
  timeout 600 swipl "$HERE/dump_comptime.pl" -- "$fixture" "$SCRATCH/$stem" \
    >/dev/null 2>"$SCRATCH/$stem.log"
  echo "$stem rc=$?"
done

python3 "$HERE/commit.py" "$SCRATCH" "$HERE"
