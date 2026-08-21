#!/usr/bin/env bash
# oracle-knip.sh -- grade the dead-module rail against knip on a TypeScript
# fixture whose every file is labelled with what each tool must say.
#
# knip and rustc are oracles in opposite directions. rustc cannot see a `pub`
# item in a lib crate; knip cannot see a module that IS imported but whose
# exports nothing ever calls, because an import is a use to a module graph.
# The rail reads called names in its DEAD bucket and resolved import edges in
# its CRAWL, so the two buckets disagree on purpose. Three label columns.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
CRATE="$HERE/fixtures/deadjs"
GLOB='v6/dl/deadcode/fixtures/deadjs/src/*.ts'
SEED='v6/dl/deadcode/fixtures/deadjs/src/index.ts'
WORK="$(mktemp -d "${TMPDIR:-/tmp}/oracle-knip.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
rc=0
say() { printf '%-6s %-26s %s\n' "$1" "$2" "$3"; }
bad() { rc=1; say FAIL "$1" "$2"; }

# knip's `json` reporter answers ONE document; its default `symbols` reporter
# answers a section header and then one path per line, which greps.
( cd "$CRATE" && npx --yes knip@6 -n --include files,unresolved \
    --no-config-hints --no-exit-code ) >"$WORK/knip.txt" 2>"$WORK/knip.err" || true
if grep -q '^Unresolved imports' "$WORK/knip.txt"; then
  echo "FAIL   knip could not resolve an import; fix the fixture"
  sed -n '/^Unresolved imports/,$p' "$WORK/knip.txt" >&2
  exit 1
fi
sed -n '/^Unused files/,/^Unresolved imports/p' "$WORK/knip.txt" \
  | grep -E '^[^ ]+\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$' | sort -u >"$WORK/knip.set" || true
touch "$WORK/knip.set"

bash "$HERE/dead-module-rail.sh" "$ROOT" "$GLOB" "$SEED" >"$WORK/rail.txt" 2>&1 \
  || { cat "$WORK/rail.txt"; echo "FAIL   rail run"; exit 1; }
# Two files in this fixture are named index.ts, so every set is keyed on the
# path under the fixture root, never on a basename.
bucket() { sed -n "/^== $1/,/^== $2/p" "$WORK/rail.txt" | sed -n 's|.*/deadjs/||p' | sort -u; }
bucket rail_dead_module        rail_unreachable >"$WORK/dead.set"
bucket rail_unreachable_module rail_unproven    >"$WORK/unreachable.set"
bucket rail_unproven_module    module_reach     >"$WORK/unproven.set"

# path                     knip dead unre why this case exists
while read -r file want_knip want_dead want_unre why; do
  [ -z "$file" ] && continue
  in_knip=no; in_dead=no; in_unre=no
  grep -qx "$file" "$WORK/knip.set" && in_knip=yes
  grep -qx "$file" "$WORK/dead.set" && in_dead=yes
  grep -qx "$file" "$WORK/unreachable.set" && in_unre=yes
  [ "$in_knip" = "$want_knip" ] || bad "$file" "knip said $in_knip, label says $want_knip"
  [ "$in_dead" = "$want_dead" ] || bad "$file" "rail dead said $in_dead, label says $want_dead"
  [ "$in_unre" = "$want_unre" ] || bad "$file" "rail unreachable said $in_unre, label says $want_unre"
  [ "$in_knip$in_dead$in_unre" = "$want_knip$want_dead$want_unre" ] && say ok "$file" "$why"
done <<'LABELS'
src/deadFile.ts             yes yes yes no module imports it and no call site names it
src/importedNeverCalled.ts  no  yes no  imported, so used to a module graph, never called
src/livePub.ts              no  no  no  imported from the entry and called
src/liveDeep.ts             no  no  no  reached from the entry through one hop
src/index.ts                no  no  no  the entry, and a declared seed
src/barrel.ts               no  no  no  zero own defs, so no bucket can name it
src/barrelTarget.ts         no  yes no  two `export * from` hops, no callee written anywhere
src/util/index.ts           no  yes no  `./util` names a directory, resolved to its index file
src/valueShelf.ts           no  yes no  a default import binds a name this file never writes
src/renamedTarget.ts        no  yes no  `export { a as b } from` publishes a name no def carries
src/aliasedTarget.ts        no  yes no  a bare specifier a tsconfig `paths` pattern claims
LABELS

missed="$(comm -23 "$WORK/knip.set" "$WORK/dead.set" || true)"
[ -z "$missed" ] || bad "subset" "knip flagged but rail missed: $(echo $missed)"

# Every name in this fixture is defined once, so the ambiguity bucket must be
# empty; a row landing there means the def plane started seeing something else.
unproven="$(cat "$WORK/unproven.set")"
[ -z "$unproven" ] || bad "unproven" "expected no ambiguous names, got: $(echo $unproven)"

[ "$rc" = 0 ] && echo "ORACLE-KNIP OK  knip=$(wc -l <"$WORK/knip.set" | tr -d ' ') dead=$(wc -l <"$WORK/dead.set" | tr -d ' ') unreachable=$(wc -l <"$WORK/unreachable.set" | tr -d ' ')"
exit "$rc"
