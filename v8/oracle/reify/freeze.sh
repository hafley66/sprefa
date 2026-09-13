#!/usr/bin/env bash
# Refreeze the reifier oracle from v7. Run from anywhere:
#
#   bash v8/oracle/reify/freeze.sh [sprefa-repo] [rev]
#
# REV defaults to f5018ad23, the sprefa main commit this port was read against.
# It is pinned rather than HEAD because a v8 feature branch can carry an older
# v7 than main, and the frozen bytes must name one tree.
#
# Stages a clean v7 from git at the FIXED path /tmp/dl8-reify-oracle so the
# committed bytes never carry a worktree path, then runs three dump modes:
#
#   compile  compile_dl7/4 over every .dl7 under v7/test/fixtures/ and
#            v7/applications/. The compiler never reaches src/3_emit, so this
#            mode is the receipt for that claim and dumps nothing.
#   test     the five v7 plunit suites that call emit_compiled/4 or
#            logical_program_rows/2. Real compiles, so one case runs 0.5-3 MB.
#   case     each oracle/reify/cases/*.pl, one authored program per diagnostic
#            reason over a two-relation checked program, 3-17 KB per case.
#
# The committed set is every authored case, every suite case under COMMIT_LIMIT
# bytes, and the smallest over-limit case of each entry kind so every entry is
# proven once against a real compile. oracle/reify/status.json records the rest.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=${1:-$(cd "$HERE/../../.." && pwd)}
REV=${2:-f5018ad23}
STAGE=/tmp/dl8-reify-oracle
SCRATCH=$STAGE/out
COMMIT_LIMIT=65536

rm -rf "$STAGE"
mkdir -p "$STAGE" "$SCRATCH"
git -C "$REPO" archive "$REV" v7 | tar -x -C "$STAGE"
export V7_DIR=$STAGE/v7
cd "$STAGE" || exit 1

for fixture in $(find "$STAGE/v7/test/fixtures" "$STAGE/v7/applications" -name '*.dl7' | sort); do
  stem=$(basename "$fixture" .dl7)
  timeout 600 swipl -q "$HERE/dump_reify.pl" -- compile "$fixture" \
    "$SCRATCH/compile-$stem" >/dev/null 2>"$SCRATCH/compile-$stem.log"
  echo "compile $stem rc=$? $(grep -o 'reify calls: [0-9]*' "$SCRATCH/compile-$stem.log")"
done

for suite in 1_entrypoints 9_dbsp_plan 15_interned_storage 16_storage_plain_field 17_dl6_interned; do
  timeout 900 swipl -q "$HERE/dump_reify.pl" -- test "$STAGE/v7/test/$suite.test.pl" \
    "$SCRATCH/suite-$suite" >/dev/null 2>"$SCRATCH/suite-$suite.log"
  echo "suite $suite rc=$? $(grep -o 'reify calls: [0-9]*' "$SCRATCH/suite-$suite.log")"
done

for authored in "$HERE"/cases/[0-9]*.pl; do
  stem=$(basename "$authored" .pl)
  timeout 120 swipl -q "$HERE/dump_reify.pl" -- case "$authored" \
    "$SCRATCH/case-$stem" >/dev/null 2>"$SCRATCH/case-$stem.log"
  echo "case $stem rc=$? $(grep -o 'reify calls: [0-9]*' "$SCRATCH/case-$stem.log")"
done

python3 - "$SCRATCH" "$HERE" "$COMMIT_LIMIT" <<'PY'
import glob, hashlib, json, os, shutil, sys
scratch, here, limit = sys.argv[1], sys.argv[2], int(sys.argv[3])

def digest(path):
    return hashlib.sha256(open(path, 'rb').read()).hexdigest()

for old in glob.glob(os.path.join(here, '*.json')):
    if os.path.basename(old) != 'status.json':
        os.remove(old)

paths = sorted(glob.glob(os.path.join(scratch, '*.json')))
seen, kept, skipped = {}, [], {}
for path in paths:
    h = digest(path)
    if h in seen:
        continue
    seen[h] = path
    entry = json.load(open(path))['entry']
    size = os.path.getsize(path)
    name = os.path.basename(path)
    if name.startswith('case-') or size <= limit:
        shutil.copyfile(path, os.path.join(here, name))
        kept.append(name)
    else:
        skipped.setdefault(entry, []).append((size, name, path))

# One real-compile case per entry kind joins the committed set: the smallest,
# so a megabyte program is proven end to end without a megabyte per entry.
for entry, rows in sorted(skipped.items()):
    rows.sort()
    size, name, path = rows[0]
    shutil.copyfile(path, os.path.join(here, name))
    kept.append(name)
    rows.pop(0)

total = sum(os.path.getsize(os.path.join(here, n)) for n in kept)
status = json.load(open(os.path.join(here, 'status.json')))
status['numbers'] = {
    'calls_dumped': len(paths),
    'byte_distinct': len(seen),
    'committed': len(kept),
    'committed_bytes': total,
    'distinct_checked_but_not_committed': sum(len(v) for v in skipped.values()),
    'not_committed_bytes_by_entry':
        {k: sum(size for size, _, _ in v) for k, v in sorted(skipped.items()) if v},
}
json.dump(status, open(os.path.join(here, 'status.json'), 'w'), indent=2)
print(f'froze {len(kept)} cases, {total/1e6:.2f} MB, {len(seen)} byte-distinct of {len(paths)}')
PY
