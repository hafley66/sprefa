#!/usr/bin/env bash
# check-imports.sh — the tsv2 import-gate law (plan header MECHANICAL GATE):
# every gen/*.ts file imports ONLY from ../runtime/ and rxjs; runtime/
# imports the named store symbols (SqlRunner, open_db, ISqlRunner) instead
# of declaring parallel machinery.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0

# Both generated trees are gated, not just the hand-carved one: gen_emitted/ is what
# the prolog emitter actually writes, so leaving it unscanned left the emitter free to
# import anything it liked (audit 2026-07-28).
for file in gen/*.ts gen_emitted/*.ts; do
  [ -e "$file" ] || continue
  specifiers=$(grep -oE 'from "[^"]+"' "$file" | sed -E 's/from "(.*)"/\1/' || true)
  while IFS= read -r specifier; do
    [ -z "$specifier" ] && continue
    if [ "$specifier" = "rxjs" ]; then continue; fi
    case "$specifier" in
      ../runtime/*) continue ;;
    esac
    echo "IMPORT GATE FAIL: $file imports \"$specifier\" (only ../runtime/ and rxjs allowed)"
    fail=1
  done <<< "$specifiers"
done

# The reuse law is about IMPORTS, so the check reads import lines only. A bare
# whole-file grep passed on a mere mention in a doc comment -- and scratchStore.ts's own
# header names both `open_db` and `SqlRunner` in prose, so deleting its two import lines
# and constructing @libsql/client directly left this gate green (audit 2026-07-28,
# reproduced). That is exactly the class-34 defect the gate exists to catch.
store_imports=$(grep -hE '^import .* from "(sprefa-store-engine|@libsql)' runtime/*.ts || true)
for symbol in "SqlRunner" "open_db" "ISqlRunner"; do
  if ! printf '%s\n' "$store_imports" | grep -q "\b$symbol\b"; then
    echo "IMPORT GATE FAIL: runtime/ has no import line binding store symbol \"$symbol\" (reuse law)"
    fail=1
  fi
done

if ! printf '%s\n' "$store_imports" | grep -q 'sprefa-store-engine'; then
  echo "IMPORT GATE FAIL: runtime/ never imports from sprefa-store-engine (reuse law)"
  fail=1
fi

# @libsql/client is the store's own dependency; runtime/ reaching it directly is the
# parallel-machinery defect the reuse law names.
if printf '%s\n' "$store_imports" | grep -q '@libsql'; then
  echo "IMPORT GATE FAIL: runtime/ imports @libsql directly (open_db/SqlRunner are the seam)"
  fail=1
fi

if [ "$fail" -eq 0 ]; then
  gen_count=$(ls gen/*.ts | wc -l | tr -d ' ')
  runtime_count=$(ls runtime/*.ts | wc -l | tr -d ' ')
  echo "import gate: OK ($gen_count gen files, $runtime_count runtime files)"
fi

exit "$fail"
