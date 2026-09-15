#!/usr/bin/env bash
# Refreeze the checker oracle from v7. Run from anywhere:
#
#   bash v8/oracle/check/freeze.sh [sprefa-repo] [rev]
#
# REV defaults to f5018ad23, the sprefa main commit this port was read against.
# It is pinned rather than HEAD because a v8 feature branch can carry an older
# v7 than main, and the frozen bytes must name one tree.
#
# Stages a clean v7 from git at the FIXED path /tmp/dl8-check-oracle so the
# committed bytes never carry a worktree path, then runs two dump modes:
#
#   corpus  compile_dl7/4 over every v7/test/fixtures/*.dl7 and
#           v7/applications/dl6/*.dl7. The prelude rides along, so each
#           check_datalog case is about 1.2 MB.
#   cases   compile_units/3 over each oracle/check/cases/*.dl7, one authored
#           program per diagnostic reason, no prelude, 6-20 KB per case.
#
# The committed set is the prelude pair once, the own-unit pair of three
# fixtures, and every authored case; oracle/check/status.json records the rest.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=${1:-$(cd "$HERE/../../.." && pwd)}
REV=${2:-f5018ad23}
STAGE=/tmp/dl8-check-oracle
SCRATCH=$STAGE/out
rm -rf "$STAGE"
mkdir -p "$STAGE/cases" "$SCRATCH"
git -C "$REPO" archive "$REV" v7 | tar -x -C "$STAGE"
cp "$HERE"/cases/*.dl7 "$STAGE/cases/"
export V7_DIR=$STAGE/v7

for fixture in "$STAGE"/v7/test/fixtures/*.dl7 "$STAGE"/v7/applications/dl6/*.dl7; do
  stem=$(basename "$fixture" .dl7)
  timeout 300 swipl "$HERE/dump_check.pl" -- "$fixture" "$SCRATCH/corpus-$stem" \
    >/dev/null 2>"$SCRATCH/corpus-$stem.log"
  echo "corpus $stem rc=$?"
done
for authored in "$STAGE"/cases/*.dl7; do
  stem=$(basename "$authored" .dl7)
  timeout 300 swipl "$HERE/dump_check.pl" -- bare "$authored" "$SCRATCH/case-$stem" \
    >/dev/null 2>"$SCRATCH/case-$stem.log"
  echo "case $stem rc=$?"
done

python3 - "$SCRATCH" "$HERE" <<'PY'
import glob, hashlib, json, os, shutil, sys
scratch, here = sys.argv[1], sys.argv[2]
# Three fixtures carry the committed corpus parity; every other fixture is
# checked by hand against the same scratch dump and counted in status.json.
COMMITTED_FIXTURES = ['2_partial', '3_type_algebra', '14_syntax_macros']

def load(path):
    b = open(path, 'rb').read()
    return hashlib.sha256(b).hexdigest(), json.loads(b), len(b)

corpus = {}
for path in sorted(glob.glob(os.path.join(scratch, 'corpus-*.json'))):
    stem, index = os.path.basename(path)[len('corpus-'):-len('.json')].rsplit('_', 1)
    h, case, size = load(path)
    corpus.setdefault(stem, []).append((int(index), h, case['entry'], path))
for rows in corpus.values():
    rows.sort()

# The first two calls of every compile are the type prelude and are identical
# across fixtures; everything after them belongs to the fixture's own unit.
prelude = {}
for stem, rows in corpus.items():
    for _, h, entry, path in rows[:2]:
        prelude.setdefault(entry, (h, path))

for old in glob.glob(os.path.join(here, '*.json')):
    if os.path.basename(old) != 'status.json':
        os.remove(old)

kept = []
def keep(name, path):
    target = os.path.join(here, name)
    shutil.copyfile(path, target)
    kept.append(os.path.basename(target))

for entry, (h, path) in sorted(prelude.items()):
    keep(f'prelude_{entry}.json', path)
prelude_hashes = {h for h, _ in prelude.values()}

checked_only = 0
for stem, rows in sorted(corpus.items()):
    own = {}
    for _, h, entry, path in rows:
        if h not in prelude_hashes:
            own.setdefault(entry, path)
    if stem in COMMITTED_FIXTURES:
        for entry, path in sorted(own.items()):
            keep(f'{stem}_{entry}.json', path)
    else:
        checked_only += len(own)

for path in sorted(glob.glob(os.path.join(scratch, 'case-*.json'))):
    keep(os.path.basename(path), path)

distinct = {load(p)[0] for p in glob.glob(os.path.join(scratch, '*.json'))}
total = sum(os.path.getsize(os.path.join(here, n)) for n in kept)
status = json.load(open(os.path.join(here, 'status.json')))
status['numbers'] = {
    'calls_dumped': len(glob.glob(os.path.join(scratch, '*.json'))),
    'byte_distinct': len(distinct),
    'committed': len(kept),
    'committed_bytes': total,
    'own_unit_cases_checked_but_not_committed': checked_only,
}
json.dump(status, open(os.path.join(here, 'status.json'), 'w'), indent=2)
print(f'froze {len(kept)} cases, {total/1e6:.2f} MB, {len(distinct)} byte-distinct in scratch')
PY
