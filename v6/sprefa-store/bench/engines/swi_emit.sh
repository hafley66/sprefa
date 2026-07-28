#!/usr/bin/env bash
# e2e prolog -> literal TS: emit the program module (ast.ts constructor
# helpers, no JSON), node runs it through the v1 engine seam (evalProgramSql),
# merge RSS.
set -uo pipefail
sdir="$(cd "$(dirname "$0")" && pwd)"
bdir="$(cd "$sdir/.." && pwd)"
root="$(cd "$bdir/../../.." && pwd)"
cd "$root"
/opt/homebrew/bin/swipl -q -l v6/prolog/src/emit_ts.pl \
  -g "emit(reach, 'v6/sprefa-store/js/src/gen/reach.gen.ts'),halt" 2>/dev/null
tmp=$(mktemp)
line=$(/usr/bin/time -l node --experimental-transform-types \
        v6/sprefa-store/js/src/bench/prolog_emit_bench.ts \
        v6/sprefa-store/js/src/gen/reach.gen.ts "$1" "$2" 2>"$tmp" | grep '^CSV,' | head -1)
rss=$(awk '/maximum resident set size/ {print $1}' "$tmp"); rm -f "$tmp"
[[ -z "$line" ]] && exit 1
echo "${line},0,$(awk -v b="${rss:-0}" 'BEGIN{printf "%.1f", b/1048576}'),N/A,N/A,N/A" >&2
