#!/usr/bin/env bash
set -u
# bash v8/oracle/comptime/verify.sh    (freeze.sh first; reads its scratch)
# Runs dl8 over EVERY dumped call, committed or not, and prints the failures.
HERE=$(cd "$(dirname "$0")" && pwd)
CRATE=$(cd "$HERE/../.." && pwd)
SCRATCH=${1:-/tmp/dl8-comptime-oracle/out}
MERGED=${2:-/tmp/dl8-comptime-verify}
cargo build --release --manifest-path "$CRATE/Cargo.toml" || exit 1
rm -rf "$MERGED"
mkdir -p "$MERGED"
python3 - "$HERE" "$SCRATCH" "$MERGED" "$CRATE/target/release/dl8" <<'PY'
import glob, importlib.util, json, os, subprocess, sys
here, scratch, merged, binary = sys.argv[1:5]
spec = importlib.util.spec_from_file_location('commit', os.path.join(here, 'commit.py'))
commit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(commit)
stems = sorted({os.path.basename(p).rsplit('-', 2)[0]
                for p in glob.glob(os.path.join(scratch, '*-*-*.json'))})
total = 0
failures = []
for stem in stems:
    for index, entry, case in commit.merged(scratch, stem):
        path = os.path.join(merged, f'{stem}-{entry}-{index}.json')
        json.dump(case, open(path, 'w'), sort_keys=True, separators=(',', ':'))
        run = subprocess.run([binary, 'comptime', path], capture_output=True, timeout=300)
        total += 1
        try:
            got = json.loads(run.stdout)
        except Exception:
            failures.append((stem, entry, index, 'no json', run.stderr[:200]))
            continue
        bad = [k for k, v in case['expected'].items() if got.get(k) != v]
        if bad:
            failures.append((stem, entry, index, 'differs', bad))
for row in failures:
    print(*row)
print(f'{total - len(failures)}/{total} dumped calls match v7')
sys.exit(1 if failures else 0)
PY
