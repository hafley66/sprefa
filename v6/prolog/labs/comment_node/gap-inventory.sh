#!/usr/bin/env bash
# gap-inventory.sh -- DELIVERABLE 2's evidence: exactly which v5 operations the
# seven golden techniques use, counted from the rail sources, against v6's
# whole expression inventory.
#
# v6's writable expression surface is `registry.pl expression/5`: eleven rows,
# five arithmetic and six comparison, ALL of them `both_int` or `same_type`.
# There is no text operation of any kind in the language. This script counts
# what the rails actually ask for so the gap is a number and not an adjective.
set -uo pipefail
LAB="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$LAB/../../../.." && pwd)"

RAILS="std/arch.dl std/suppress.dl examples/gen-readme.dl examples/gen-lang-skill.dl examples/gen-plans-index.dl examples/gen-zone-info.dl examples/lint-unwrap.dl"
OPS='=~|replace_re|trim\(|split\(|int\(|concat\(|lines\(|jsonp?\(|lower\(|upper\(|starts_with|edit\(|write\(|count\(|min\(|max\(|match_line\(|match_ast\('

printf '%-34s %s\n' 'v5 rail' 'operations used'
total=0
for rail in $RAILS; do
  used="$(grep -oE "$OPS" "$ROOT/$rail" | sort | uniq -c | sort -rn | awk '{printf "%s x%s  ", $2, $1}')"
  n="$(grep -coE "$OPS" "$ROOT/$rail")"
  total=$((total + n))
  printf '%-34s %s\n' "$rail" "$used"
done
echo
echo "total operation call sites across the seven rails: $total"
echo
echo "v6 expression inventory (registry.pl expression/5):"
grep -E '^expression\(' "$ROOT/v6/prolog/compile/registry.pl" | sed 's/^/  /'
echo
echo "text operations in that inventory: $(grep -cE "^expression\(.*(text|string|regex|split|replace)" "$ROOT/v6/prolog/compile/registry.pl")"
