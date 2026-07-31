#!/usr/bin/env bash
# check-imports.sh — tsv2 import gate:
# every gen/*.ts file imports ONLY from ../runtime/ and rxjs; runtime/
# imports the named store symbols (SqlRunner, open_db, ISqlRunner) instead
# of declaring parallel machinery.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0

# Both generated trees are gated.
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
# serve/ is gated too, on its own (wider) allowlist: it is an APPLICATION layer,
# so node builtins and the two bought libraries it names are legal there, while
# the class-34 rule that matters stays enforced -- it may not reach @libsql
# directly (open_db/SqlRunner are the seam), and it may not import v6/dl, whose
# runtime is the sibling this arc deliberately did NOT couple to.
for file in serve/*.ts; do
  [ -e "$file" ] || continue
  specifiers=$(grep -oE 'from "[^"]+"' "$file" | sed -E 's/from "(.*)"/\1/' || true)
  while IFS= read -r specifier; do
    [ -z "$specifier" ] && continue
    case "$specifier" in
      rxjs|pino) continue ;;
      node:*) continue ;;
      @noble/hashes/*) continue ;;
      ./*|../runtime/*) continue ;;
      sprefa-store-engine/*) continue ;;
    esac
    echo "IMPORT GATE FAIL: $file imports \"$specifier\" (serve/ allows node:, rxjs, pino, @noble/hashes, ./, ../runtime/, sprefa-store-engine/)"
    fail=1
  done <<< "$specifiers"
done

serve_imports=$(grep -hE '^import .* from "' serve/*.ts 2>/dev/null || true)
if printf '%s\n' "$serve_imports" | grep -q '@libsql'; then
  echo "IMPORT GATE FAIL: serve/ imports @libsql directly (open_db/SqlRunner are the seam)"
  fail=1
fi
if printf '%s\n' "$serve_imports" | grep -qE '\.\./\.\./dl/'; then
  echo "IMPORT GATE FAIL: serve/ imports v6/dl (the sibling runtime stays uncoupled)"
  fail=1
fi

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
  serve_count=$(ls serve/*.ts 2>/dev/null | wc -l | tr -d ' ')
  echo "import gate: OK ($gen_count gen files, $runtime_count runtime files, $serve_count serve files)"
fi

exit "$fail"
