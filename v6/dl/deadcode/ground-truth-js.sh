#!/usr/bin/env bash
# ground-truth-js.sh -- grade the dead-module rail against knip on a TypeScript
# fixture whose every file is labelled with what each tool must say.
#
# knip and rustc are oracles in opposite directions. rustc cannot see a `pub`
# item in a lib crate; knip cannot see a module that IS imported but whose
# exports nothing ever calls, because an import is a use to a module graph.
# The rail reads called names, so it sees both. Each label states which.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
CRATE="$HERE/fixtures/deadjs"
GLOB='v6/dl/deadcode/fixtures/deadjs/src/*.ts'
SEED='v6/dl/deadcode/fixtures/deadjs/src/index.ts'
WORK="$(mktemp -d "${TMPDIR:-/tmp}/ground-truth-js.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
rc=0
say() { printf '%-6s %-26s %s\n' "$1" "$2" "$3"; }
bad() { rc=1; say FAIL "$1" "$2"; }

( cd "$CRATE" && npx --yes knip@6 --reporter json ) >"$WORK/knip.json" 2>"$WORK/knip.err" || true
python3 - "$WORK/knip.json" >"$WORK/knip.set" <<'PY' || { echo "FAIL   knip produced no usable report"; cat "$WORK/knip.err" >&2; exit 1; }
import json,sys,os
d=json.load(open(sys.argv[1]))
for issue in d["issues"]:
    if issue["unresolved"]:
        raise SystemExit(f"knip could not resolve an import in {issue['file']}; fix the fixture")
    for f in issue["files"]:
        print(os.path.basename(f["name"]))
PY
sort -u -o "$WORK/knip.set" "$WORK/knip.set"

bash "$HERE/dead-module-rail.sh" "$ROOT" "$GLOB" "$SEED" >"$WORK/rail.txt" 2>&1 \
  || { cat "$WORK/rail.txt"; echo "FAIL   rail run"; exit 1; }
sed -n '/^== rail_unproven_module/,/^== module_reach/p' "$WORK/rail.txt" \
  | grep -oE '[A-Za-z_]+\.(rs|ts)$' | sort -u >"$WORK/unproven.set" || true
touch "$WORK/unproven.set"
sed -n '/^== rail_dead_module/,/^== rail_unreachable/p' "$WORK/rail.txt" \
  | grep -oE '[A-Za-z_]+\.ts$' | sort -u >"$WORK/rail.set" || true
touch "$WORK/rail.set"

# file                  knip rail  why this case exists
while read -r file want_knip want_rail why; do
  [ -z "$file" ] && continue
  in_knip=no; in_rail=no
  grep -qx "$file" "$WORK/knip.set" && in_knip=yes
  grep -qx "$file" "$WORK/rail.set" && in_rail=yes
  [ "$in_knip" = "$want_knip" ] || bad "$file" "knip said $in_knip, label says $want_knip"
  [ "$in_rail" = "$want_rail" ] || bad "$file" "rail said $in_rail, label says $want_rail"
  [ "$in_knip$in_rail" = "$want_knip$want_rail" ] && say ok "$file" "$why"
done <<'LABELS'
deadFile.ts             yes yes no module imports it and no call site names it
importedNeverCalled.ts  no  yes imported, so used to a module graph, never called
livePub.ts              no  no  imported from the entry and called
liveDeep.ts             no  no  reached from the entry through one hop
index.ts                no  no  the entry, and a declared seed
LABELS

missed="$(comm -23 "$WORK/knip.set" "$WORK/rail.set" || true)"
[ -z "$missed" ] || bad "subset" "knip flagged but rail missed: $(echo $missed)"

[ "$rc" = 0 ] && echo "GROUND-TRUTH-JS OK  knip=$(wc -l <"$WORK/knip.set" | tr -d ' ') rail=$(wc -l <"$WORK/rail.set" | tr -d ' ')"
exit "$rc"
