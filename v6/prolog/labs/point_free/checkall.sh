#!/usr/bin/env bash
# checkall.sh : every corpus program through BOTH doors, one line each.
# Oracle leg reports the tick count; text door reports the exit code.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
for program in "$HERE"/today/*.dl6 "$HERE"/out/*.dl6; do
  name="$(basename "$program" .dl6)"
  schedule="$HERE/today/$name.schedule.json"
  [ -f "$schedule" ] || { printf '%-40s NO SCHEDULE\n' "$name"; continue; }
  ticks="$( cd "$REPO/v6/prolog/compile/scripts" \
    && swipl -q -l dl6_oracle.pl -g "oracle('$program','$schedule')" -g halt 2>/dev/null | wc -l | tr -d ' ' )"
  out="$( cd "$REPO/v6/tsv2" && npm run --silent bop -- check "$program" 2>&1 )"
  code=$?
  refusal="$(printf '%s' "$out" | grep -o 'refusal:.*' | head -1)"
  printf '%-24s %-10s ticks=%-4s check=%s %s\n' \
    "$name" "$(basename "$(dirname "$program")")" "$ticks" "$code" "$refusal"
done
