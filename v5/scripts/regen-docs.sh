#!/usr/bin/env bash
# Regenerate every self-hosted doc: the gen-*.dl reference/index programs and
# the README zone splicers. Adding a generator = one line in the list below.
#
# Encoded gotchas (learned the slow way):
# - A running daemon built from an OLDER binary re-ticks on these writes and
#   can silently revert a regen. We warn and run with DL_NO_DAEMON=1; if a daemon is
#   up, eyeball `git diff --stat` again a few seconds after this exits.
# - A warm db from an older schema misses rows (first gen pass looks
#   incomplete). Every generator gets a FRESH --db.
# - Generators converge by design (no write when content matches). A second
#   pass that still changes the tree is a generator bug: fail loudly, never
#   hand-patch the rendered output (BEGIN/END zones are machine-owned; fix
#   the generator).
set -euo pipefail
cd "$(dirname "$0")/.."

if pgrep -f 'dl --daemon' >/dev/null 2>&1; then
  echo "[regen-docs] WARNING: a dl daemon is running; re-check git diff after exit." >&2
fi

dl="${DL_BIN:-target/debug/dl}"
[ -x "$dl" ] || { echo "[regen-docs] building dl"; cargo build --bin dl; }

gens=(
  examples/gen-reference.dl
  examples/gen-doc-indexes.dl
  examples/gen-skill-ref.dl
  examples/gen-type-table.dl
  examples/gen-doc-index.dl
  examples/gen-engine-anchors.dl
  examples/builtin-rels.dl
  examples/op-table.dl
)

for g in "${gens[@]}"; do
  db="$(mktemp -d)/regen.sqlite"
  echo "[regen-docs] $g"
  DL_NO_DAEMON=1 "$dl" "$g" --db "$db"
  pass1=$(git diff | shasum | cut -d' ' -f1)
  DL_NO_DAEMON=1 "$dl" "$g" --db "$db"
  pass2=$(git diff | shasum | cut -d' ' -f1)
  if [ "$pass1" != "$pass2" ]; then
    echo "[regen-docs] $g did NOT converge on second pass — generator bug, fix it (never the output)" >&2
    exit 1
  fi
done

echo "[regen-docs] checked-claims rail (skill cites must resolve)"
DL_NO_DAEMON=1 "$dl" examples/gen-skill-ref.dl --db "$(mktemp -d)/rail.sqlite" --check

echo "[regen-docs] drift:"
git diff --stat
