#!/usr/bin/env bash
# @comment-ok: the two-language contract below is what the gate asserts
# Go-to-reference over both module systems. Prolog rows come from
# sprefa-extract's CallF specifiers; dl6 rows from the compiler's own
# use-resolution, since a .dl6 module graph is minted at compile time and never
# needs an extractor.
#
# Pinned counts are the CONTRACT, not a snapshot: a prolog import that stops
# resolving, or a dl6 rel that loses its declaring module, moves one of them.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v6="$(cd "$here/.." && pwd)"
root="$(cd "$v6/.." && pwd)"
status=0

extract_bin="$v6/sprefa-extract/target/debug/extract"
if [ ! -x "$extract_bin" ]; then
  cargo build --quiet --features cli --manifest-path "$v6/sprefa-extract/Cargo.toml"
fi

specs="$(mktemp)"
trap 'rm -f "$specs"' EXIT
cd "$root"
for file in $(git ls-files 'v6/prolog' | grep '\.pl$'); do
  "$extract_bin" --family call "$file" 2>/dev/null \
    | python3 -c "
import sys, json
for line in sys.stdin:
    row = json.loads(line)
    if row.get('record') == 'specifier':
        row['path'] = '$file'
        print(json.dumps(row))
" >>"$specs"
done

python3 - "$specs" <<'PY'
import collections, json, os, posixpath, sys

rows = [json.loads(line) for line in open(sys.argv[1])]
tracked = {path for path in os.popen("git ls-files 'v6/prolog'").read().split()
           if path.endswith(".pl")}


def resolve(importer, specifier):
    text = specifier.strip("'\"")
    if text.startswith("library("):
        return None
    joined = posixpath.normpath(posixpath.join(posixpath.dirname(importer), text))
    for candidate in (joined, joined + ".pl"):
        if candidate in tracked:
            return candidate
    return None


exports = {(row["path"], row["name"]) for row in rows if row["kind"] == "reexport"}
references = collections.defaultdict(list)
unresolved = 0
for row in rows:
    if row["kind"] != "named":
        continue
    target = resolve(row["path"], row["module"])
    if target is None:
        unresolved += 1
        continue
    references[(target, row["name"])].append(row["path"])

named = sum(1 for row in rows if row["kind"] == "named")
dangling = sorted(pair for pair in references if pair not in exports)
print(f"PROLOG_XREF files={len(tracked)} exports={len(exports)} "
      f"imports={named} resolved={named - unresolved} "
      f"unused_exports={len(exports) - len([p for p in exports if references.get(p)])} "
      f"dangling={len(dangling)}")
for path, name in dangling:
    print(f"  DANGLING {path}:{name} imported but not exported")
sys.exit(1 if dangling else 0)
PY
prolog_status=$?
[ "$prolog_status" = 0 ] || status=1

cd "$here"
swipl -q -l xref.pl -g "dl6_report('xref_fixtures/root.dl6')" -g halt
exit "$status"
