import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

here=Path(__file__).resolve().parent
run=Path(tempfile.mkdtemp(prefix='gate-',dir='/tmp/sprefa-sqlite-competitive'))
env=dict(os.environ,TAKE2_EXTENSION=str(run/'competitive.dylib'),TAKE2_SCRATCH=str(run),IVM_RUN_ROOT=str(run/'shared'))
env.update(CARGO_HOME='/tmp/sprefa-sqlite-competitive/cargo',CARGO_TARGET_DIR='/tmp/sprefa-sqlite-competitive/compiler-target',TAKE2_COMPILER='/tmp/sprefa-sqlite-competitive/compiler-target/debug/take2-sql-compile')
commands=[['cc','-O2','-Wall','-Wextra','-Werror','-fPIC','-shared','-I/opt/homebrew/opt/sqlite/include',str(here/'1_native.c'),'-o',env['TAKE2_EXTENSION']],
          [sys.executable,str(here.parent/'42_sqlite_native_take2/3_boundary_test.py')],
          [sys.executable,str(here.parent/'42_sqlite_native_take2/6_semantic_test.py')],
          [sys.executable,str(here/'3_batch_test.py')],
          [sys.executable,str(here/'3_batch_test.py')],
          [sys.executable,str(here/'3a_circuit_test.py')],
          [sys.executable,str(here/'3a_circuit_test.py')],
          [sys.executable,str(here/'3b_frontier_test.py')],
          [sys.executable,str(here/'3b_frontier_test.py')],
          ['node',str(here.parent/'12_crossover_runner.mjs'),'--profile','semantic','--arms','sqlite-competitive-batch,sqlite-competitive-sourceview,sqlite-competitive-lazy','--competitive-extension',env['TAKE2_EXTENSION'],'--output',str(run/'shared.jsonl'),'--repetitions','1','--warmups','0']]
commands.append(['node',str(here.parent/'12_crossover_runner.mjs'),'--profile','circuits','--circuits','pipeline,join,self_join,chain,semijoin,antijoin,reach_cycle,aggregate_churn,distinct,fanout_fanin,diamond','--arms','sqlite-competitive-batch,sqlite-competitive-sourceview,sqlite-competitive-frontier,sqlite-competitive-lazy','--competitive-extension',env['TAKE2_EXTENSION'],'--output',str(run/'circuits.jsonl'),'--repetitions','1','--warmups','0'])
commands.append([sys.executable,str(here/'3c_cache_test.py')])
commands.append([sys.executable,str(here/'2a_plan_audit.py'),env['TAKE2_EXTENSION'],str(run/'plans.db')])
commands.append([sys.executable,str(here/'3d_lazy_test.py')])
commands.append([sys.executable,str(here/'3d_lazy_test.py')])
commands.append(['cargo','test','--locked','--manifest-path',str(here/'6_sql_compile/Cargo.toml')])
commands.append(['cargo','build','--locked','--manifest-path',str(here/'6_sql_compile/Cargo.toml')])
commands.append([sys.executable,str(here/'7_compile_test.py')])
commands.append([sys.executable,str(here/'7_compile_test.py')])
steps=[];rc=0
for index,command in enumerate(commands):
    with (run/f'{index}.log').open('wb') as log:
        try:rc=subprocess.run(command,env=dict(env,TAKE2_SOURCE_VIEWS='1') if index in [4,6,8,14,18] else env,stdout=log,stderr=subprocess.STDOUT,timeout=120).returncode
        except subprocess.TimeoutExpired:rc=124
    steps.append(dict(command=command,exit_code=rc))
    if rc:break
if rc==0:
    rows=[json.loads(s) for s in (run/'shared.jsonl').read_text().splitlines()]
    if not any(r['event']=='all-arm-run' and r.get('all_input_output_states_match') and r.get('state_count_per_arm')==171 for r in rows):rc=1
    circuits=[json.loads(s) for s in (run/'circuits.jsonl').read_text().splitlines()]
    paired=[r for r in circuits if r['event']=='all-arm-run']
    if len(paired)!=11 or not all(r.get('all_input_output_states_match') and r.get('state_count_per_arm')==13 and len(r['arms'])==4 and not r['excluded_arms'] for r in paired):rc=1
    if sum(r['event']=='frontier-check' and r.get('completion_after_all_inputs_advanced') for r in circuits)!=11:rc=1
hashes={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [*here.glob('*'),*here.glob('6_sql_compile/*'),*run.glob('*')] if p.is_file()}
(run/'receipt.json').write_text(json.dumps(dict(exit_code=rc,steps=steps,hashes=hashes),indent=2))
print(json.dumps(dict(receipt=str(run/'receipt.json'),exit_code=rc)));sys.exit(rc)
