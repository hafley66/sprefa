#!/usr/bin/env bash
set -u
# bash v8/oracle/compile/freeze.sh [sprefa-repo] [rev]
# REV is pinned, never HEAD: a v8 branch can carry an older v7 than main.
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=${1:-$(cd "$HERE/../../.." && pwd)}
REV=${2:-f5018ad23}
STAGE=/tmp/dl8-compile-oracle
SCRATCH=$STAGE/out
rm -rf "$STAGE"
mkdir -p "$SCRATCH"
git -C "$REPO" archive "$REV" v7 | tar -x -C "$STAGE"
export V7_DIR=$STAGE/v7
cd "$STAGE/v7" || exit 1

dump() {
  local stem=$1; shift
  local args=()
  printf '%s\n' "$@" > "$SCRATCH/$stem.args"
  for token in "$@"; do
    case "$token" in
      --project|--tsi) args+=("$token") ;;
      *) args+=("$STAGE/v7/$token") ;;
    esac
  done
  local start=$SECONDS
  local before after
  before=$(python3 -c 'import time;print(time.time())')
  timeout 600 swipl "$HERE/dump_compile.pl" -- "$SCRATCH" "$stem" "${args[@]}" \
    >/dev/null 2>"$SCRATCH/$stem.log"
  local rc=$?
  after=$(python3 -c 'import time;print(time.time())')
  python3 -c "print(round(($after-$before)*1000))" > "$SCRATCH/$stem.ms"
  echo "$stem rc=$rc $(cat "$SCRATCH/$stem.ms")ms"
  : "$start"
}

for fixture in $(find test/fixtures applications -name '*.dl7' | sort); do
  stem=${fixture%.dl7}
  dump "${stem//\//-}" "$fixture"
done

while IFS=$'\t' read -r stem rest; do
  [ -z "$stem" ] && continue
  IFS=$'\t' read -r -a tokens <<< "$rest"
  dump "$stem" "${tokens[@]}"
done < "$HERE/cases.txt"

python3 "$HERE/commit.py" "$SCRATCH" "$HERE" "$STAGE/v7"
