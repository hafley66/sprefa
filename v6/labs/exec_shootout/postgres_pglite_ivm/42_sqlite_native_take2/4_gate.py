"""Reproducible bounded build/test gate. Every child exit and artifact hash is saved."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
scratch = Path(os.environ.get('TAKE2_SCRATCH', '/tmp/sprefa-sqlite-native-take2'))
scratch.mkdir(parents=True, exist_ok=True)
run = Path(tempfile.mkdtemp(prefix='gate-', dir=scratch))
env = dict(os.environ, TAKE2_SCRATCH=str(scratch), TAKE2_EXTENSION=str(run / 'take2.dylib'))
steps = []

def execute(name, command):
    start = time.monotonic()
    with (run / (name + '.log')).open('wb') as log:
        try:
            result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=120)
            rc = result.returncode
        except subprocess.TimeoutExpired:
            rc = 124
    steps.append(dict(name=name, command=command, exit_code=rc, seconds=time.monotonic()-start))
    return rc

rc = execute('build', ['cc','-Wall','-Wextra','-Werror','-fPIC','-shared',
    '-I' + os.environ.get('TAKE2_SQLITE_INCLUDE','/opt/homebrew/opt/sqlite/include'),
    str(HERE/'1_native.c'), '-o', env['TAKE2_EXTENSION']])
if rc == 0:
    rc = execute('boundary', [sys.executable,str(HERE/'3_boundary_test.py')])
if rc == 0:
    rc = execute('semantic', [sys.executable,str(HERE/'6_semantic_test.py')])
if rc == 0:
    env['IVM_RUN_ROOT']=str(run/'shared-cases')
    rc=execute('shared-semantic',['node',str(HERE.parent/'12_crossover_runner.mjs'),
        '--profile','semantic','--arms','sqlite-native-take2,sqlite-native-take2-logged',
        '--take2-extension',env['TAKE2_EXTENSION'],'--output',str(run/'shared.jsonl'),
        '--warmups','0','--repetitions','1'])
    if rc==0:
        rows=[json.loads(line) for line in (run/'shared.jsonl').read_text().splitlines()]
        matches=[r for r in rows if r['event']=='all-arm-run' and r['status']=='ok'
                 and r.get('all_input_output_states_match') and r.get('state_count_per_arm')==171]
        if len(matches)!=1: rc=1
        steps.append(dict(name='shared-executed-oracle',exit_code=rc,matched_cases=len(matches)))
hashes = {}
for path in sorted(HERE.glob('*')) + sorted(run.glob('*')) + [HERE.parent/name for name in ['9_crossover_workload.mjs','12_crossover_runner.mjs','13_crossover_run.sh']]:
    if path.is_file(): hashes[str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()
receipt = dict(exit_code=rc, steps=steps, sha256=hashes)
(run/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(dict(receipt=str(run/'receipt.json'),exit_code=rc)),flush=True)
sys.exit(rc)
