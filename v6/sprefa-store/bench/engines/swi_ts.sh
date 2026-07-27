#!/usr/bin/env bash
# e2e prolog->TypeScript: regenerate the engine, run under node, merge RSS.
set -uo pipefail
sdir="$(cd "$(dirname "$0")" && pwd)"
bdir="$(cd "$sdir/.." && pwd)"
root="$(cd "$bdir/../../.." && pwd)"
cd "$root"
/opt/homebrew/bin/swipl -q -l books/v6/dl_to_ts.pl -l "$bdir/ts_reach_gen.pl" \
  -g 'gen(bench_reach),halt' 2>/dev/null
tmp=$(mktemp)
line=$(/usr/bin/time -l node books/v6/gen/bench_reach.ts "$1" "$2" 2>"$tmp" | grep '^CSV,' | head -1)
rss=$(awk '/maximum resident set size/ {print $1}' "$tmp"); rm -f "$tmp"
[[ -z "$line" ]] && exit 1
echo "${line},0,$(awk -v b="${rss:-0}" 'BEGIN{printf "%.1f", b/1048576}')" >&2
