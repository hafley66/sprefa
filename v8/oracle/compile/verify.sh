#!/usr/bin/env bash
set -u
# bash v8/oracle/compile/verify.sh    (freeze.sh first; reads its scratch)
# Runs dl8 over EVERY case, committed or not, and prints the failures.
HERE=$(cd "$(dirname "$0")" && pwd)
CRATE=$(cd "$HERE/../.." && pwd)
SCRATCH=${1:-/tmp/dl8-compile-oracle/out}
cargo build --release --manifest-path "$CRATE/Cargo.toml" || exit 1
python3 - "$HERE" "$SCRATCH" "$CRATE/target/release/dl8" <<'PY'
import json, os, subprocess, sys, time
here, scratch, binary = sys.argv[1:4]
sources = os.path.join(here, 'sources')
status = json.load(open(os.path.join(here, 'status.json')))
v7 = '/tmp/dl8-compile-oracle/v7'
failures, rows = [], []
for case in status['cases']:
    stem = case['stem']
    dump = os.path.join(scratch, stem + '.json')
    if not os.path.exists(dump):
        failures.append((stem, 'no dump'))
        continue
    expected = json.loads(open(dump, encoding='utf-8').read().replace(v7, '<root>'))
    arguments = [a if a.startswith('--') else a.replace('<root>', sources)
                 for a in case['arguments']]
    start = time.time()
    run = subprocess.run([binary, 'compile'] + arguments, capture_output=True, timeout=300)
    ms = round((time.time() - start) * 1000)
    rows.append((stem, ms, case['v7_ms']))
    got = run.stdout.decode('utf-8').replace(sources, '<root>')
    if got.strip() != json.dumps(expected, sort_keys=True, separators=(',', ':')):
        try:
            bad = [k for k, v in expected.items() if json.loads(got).get(k) != v]
        except Exception:
            bad = ['no json: ' + run.stderr.decode()[:200]]
        failures.append((stem, bad))
    want = 1 if expected['diagnostics'] else 0
    if run.returncode != want:
        failures.append((stem, f'exit {run.returncode} want {want}'))
print(f'{"case":58} {"dl8 ms":>7} {"v7 ms":>7}')
for stem, ms, v7ms in rows:
    print(f'{stem:58} {ms:>7} {v7ms:>7}')
for row in failures:
    print('FAIL', *row)
print(f'{len(rows) - len({f[0] for f in failures})}/{len(status["cases"])} cases match v7')
sys.exit(1 if failures else 0)
PY
