#!/usr/bin/env bash
# Refreeze the loader oracle from v7.
#
#   bash v8/oracle/load/freeze.sh [scratch-dir]
#
# Run from the v8 worktree root. SPREFA_DIR names the tree that holds the v7
# oracle (default: ../../sprefa relative to this worktree); its own root is the
# working directory for the corpus drivers because v7's fixture paths are
# spelled relative to it.
#
# Three drivers. `tests` and `compile` run inside the sprefa tree and record
# every loader call v7 makes while running its own plunit files and compiling
# three projects. `probes` runs here and records one crafted call per
# diagnostic code the corpus does not reach.
#
# Cases are then deduplicated by content and named by content, so a refreeze
# that changes nothing rewrites nothing. Cases over the size ceiling are
# verified locally and listed in status.json instead of being committed.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
SCRATCH=${1:-$(mktemp -d)}
SPREFA=${SPREFA_DIR:-$(cd "$ROOT/../../sprefa" && pwd)}
V7="$SPREFA/v7"
CEILING=${CEILING:-200000}
mkdir -p "$SCRATCH"

for driver in tests compile; do
  ( cd "$SPREFA" && V7_DIR="$V7" timeout 600 swipl "$HERE/dump_load.pl" -- \
      "$driver" "$SCRATCH" >/dev/null 2>"$SCRATCH/$driver.log" )
  echo "$driver rc=$?"
done
( cd "$ROOT" && V7_DIR="$V7" timeout 300 swipl "$HERE/dump_load.pl" -- \
    probes "$SCRATCH" >/dev/null 2>"$SCRATCH/probes.log" )
echo "probes rc=$?"

python3 - "$SCRATCH" "$HERE" "$CEILING" <<'PY'
import glob, hashlib, json, os, re, shutil, sys

scratch, here, ceiling = sys.argv[1], sys.argv[2], int(sys.argv[3])
for old in glob.glob(os.path.join(here, '*.json')):
    if os.path.basename(old) != 'status.json':
        os.remove(old)

seen, cases = {}, []
for path in sorted(glob.glob(os.path.join(scratch, '*.json'))):
    body = open(path, 'rb').read()
    digest = hashlib.sha1(body).hexdigest()
    if digest in seen:
        continue
    seen[digest] = path
    call = json.loads(body)['input']['call']
    cases.append((call, digest, len(body), path))

committed, held = [], []
for call, digest, size, path in sorted(cases):
    name = f'{call}_{digest[:8]}.json'
    if size <= ceiling:
        shutil.copyfile(path, os.path.join(here, name))
        committed.append(name)
    else:
        held.append({'case': name, 'call': call, 'bytes': size})

# The largest held case of each call still ships, so one real-project shape of
# every install is inside the gate.
for call in sorted({row['call'] for row in held}):
    biggest = max((r for r in held if r['call'] == call),
                  key=lambda r: r['bytes'])
    source = seen[[d for _, d, _, _ in cases
                   if f'{call}_{d[:8]}.json' == biggest['case']][0]]
    shutil.copyfile(source, os.path.join(here, biggest['case']))
    committed.append(biggest['case'])
    held.remove(biggest)

status = json.load(open(os.path.join(here, 'status.json')))
status['skip'] = []
status['committed'] = sorted(committed)
status['checked_not_committed'] = sorted(held, key=lambda r: r['case'])
json.dump(status, open(os.path.join(here, 'status.json'), 'w'),
          indent=2, sort_keys=True)
open(os.path.join(here, 'status.json'), 'a').write('\n')

total = sum(os.path.getsize(os.path.join(here, n)) for n in committed)
print(f'dumped {len(list(glob.glob(os.path.join(scratch, "*.json"))))} calls, '
      f'{len(cases)} distinct, {len(committed)} committed, '
      f'{total / 1e6:.2f} MB, {len(held)} held')
PY
