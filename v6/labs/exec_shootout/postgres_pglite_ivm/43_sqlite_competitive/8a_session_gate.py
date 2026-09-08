"""Task-owned session probe and pinned upstream session regression tests."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

here = Path(__file__).resolve().parent
build = Path(sys.argv[1]).resolve()
source = Path('/tmp/sprefa-sqlite-extension-research.ZsALrZ/sqlite')
assert subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip() == 'f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9'
run = Path(tempfile.mkdtemp(prefix='session-gate-', dir='/tmp/sprefa-sqlite-competitive'))
env = dict(os.environ, DYLD_LIBRARY_PATH=str(build))
commands = [
    ['cc', '-g', '-O1', '-fsanitize=address', '-Wall', '-Wextra', '-Werror', '-I'+str(build), str(here/'8_session_probe.c'), str(build/'libsqlite3.dylib'), '-Wl,-rpath,'+str(build), '-o', str(run/'probe')],
    [str(run/'probe'), str(run/'state.db')],
    [str(build/'testfixture'), str(source/'ext/session/session1.test')],
    [str(build/'testfixture'), str(source/'ext/session/session2.test')],
]
steps = []
rc = 0
for index, command in enumerate(commands):
    with (run/f'{index}.log').open('w') as log:
        try:
            rc = subprocess.run(command, cwd=run, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=120).returncode
        except subprocess.TimeoutExpired:
            rc = 124
    steps.append(dict(command=command, exit_code=rc))
    if rc:
        break
hashes = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in [here/'8_session_probe.c', here/'8a_session_gate.py', build/'libsqlite3.dylib', *run.glob('*.log')]}
(run/'receipt.json').write_text(json.dumps(dict(exit_code=rc, steps=steps, hashes=hashes), indent=2))
print(json.dumps(dict(receipt=str(run/'receipt.json'), exit_code=rc)))
sys.exit(rc)
