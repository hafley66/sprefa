#!/usr/bin/env bash
# Refreeze the lowering oracle from v7. Run from the sprefa repository root
# (v7 loads its prelude relative to it):
#
#   V7_DIR=v7 bash v8/oracle/lower/freeze.sh [scratch-dir]
#
# One swipl run per v7/test/fixtures/*.dl7 dumps every lower_datalog/5 and
# lower_datalog_deferred/5 call of that compile into the scratch directory.
# The committed set is then curated: the two macrotime lowerings (identical for
# every fixture) once, plus the first deferred lowering of each fixture's own
# unit. The prelude lowerings and the strict re-lowerings are 40 MB of near
# duplicates and stay out of git; oracle/lower/status.json lists them.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
SCRATCH=${1:-$(mktemp -d)}
V7=${V7_DIR:-v7}
mkdir -p "$SCRATCH"
for fixture in "$V7"/test/fixtures/*.dl7; do
  stem=$(basename "$fixture" .dl7)
  timeout 300 swipl "$HERE/dump_lower.pl" -- "$fixture" "$SCRATCH/$stem" \
    >/dev/null 2>"$SCRATCH/$stem.log"
  echo "$stem rc=$?"
done
python3 - "$SCRATCH" "$HERE" <<'PY'
import glob, json, os, shutil, sys
scratch, here = sys.argv[1], sys.argv[2]
macrotime = {}
kept = []
first_deferred = {}
for path in sorted(glob.glob(os.path.join(scratch, '*.json'))):
    case = json.load(open(path))
    origin = case['input']['origin']
    policy = case['input']['policy']
    name = origin.get('a') if isinstance(origin, dict) and 'a' in origin else None
    stem = os.path.basename(path).rsplit('_', 1)[0]
    if name == 'macrotime':
        macrotime.setdefault(policy, path)
    elif name is None and policy == 'defer_unknown_calls':
        first_deferred.setdefault(stem, path)
for policy, path in sorted(macrotime.items()):
    target = os.path.join(here, f'macrotime_{policy}.json')
    shutil.copyfile(path, target)
    kept.append(os.path.basename(target))
for stem, path in sorted(first_deferred.items()):
    target = os.path.join(here, f'{stem}_deferred.json')
    shutil.copyfile(path, target)
    kept.append(os.path.basename(target))
total = sum(os.path.getsize(os.path.join(here, n)) for n in kept)
print(f'froze {len(kept)} cases, {total/1e6:.1f} MB')
PY
