#!/usr/bin/env bash
# check-imports.sh — the tsv2 import-gate law (plan header MECHANICAL GATE):
# every gen/*.ts file imports ONLY from ../runtime/ and rxjs; runtime/
# imports the named store symbols (SqlRunner, open_db, ISqlRunner) instead
# of declaring parallel machinery.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0

for file in gen/*.ts; do
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

for symbol in "SqlRunner" "open_db" "ISqlRunner"; do
  if ! grep -rq "$symbol" runtime/*.ts; then
    echo "IMPORT GATE FAIL: runtime/ never references store symbol \"$symbol\" (reuse law)"
    fail=1
  fi
done

if ! grep -rq 'sprefa-store-engine' runtime/*.ts; then
  echo "IMPORT GATE FAIL: runtime/ never imports from sprefa-store-engine (reuse law)"
  fail=1
fi

if [ "$fail" -eq 0 ]; then
  gen_count=$(ls gen/*.ts | wc -l | tr -d ' ')
  runtime_count=$(ls runtime/*.ts | wc -l | tr -d ' ')
  echo "import gate: OK ($gen_count gen files, $runtime_count runtime files)"
fi

exit "$fail"
