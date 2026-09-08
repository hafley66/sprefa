"""Pinned out-of-tree SQLite debug build; no installation or source mutation."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

source=Path('/tmp/sprefa-sqlite-extension-research.ZsALrZ/sqlite')
sha='f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9'
assert subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()==sha
root=Path(tempfile.mkdtemp(prefix='sqlite-debug-',dir='/tmp/sprefa-sqlite-competitive'))
steps=[]
def run(command,timeout=300,env=None):
 index=len(steps)
 with (root/f'{index}.log').open('wb') as log:
  try:rc=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=timeout).returncode
  except subprocess.TimeoutExpired:rc=124
 steps.append(dict(command=list(map(str,command)),exit_code=rc))
 (root/'receipt.json').write_text(json.dumps(dict(source_sha=sha,source_license='public domain',source_patch=None,steps=steps,exit_code=rc),indent=2))
 print(json.dumps(dict(root=str(root),step=index,exit_code=rc)),flush=True)
 if rc:raise SystemExit(rc)
run([str(source/'configure'),'--fts5','--session','--debug','--with-tcl=/opt/homebrew/opt/tcl-tk@8/lib'])
run(['make','-j2','sqlite3','libsqlite3.dylib','testfixture'],600)
for name in ['savepoint.test','savepoint2.test','conflict.test','conflict2.test']:
 run([str(root/'testfixture'),str(source/'test'/name)])
for name in ['fts5savepoint.test','fts5conflict.test']:
 run([str(root/'testfixture'),str(source/'ext/fts5/test'/name)])
library=root/'libsqlite3.0.dylib'
if not library.exists():library.symlink_to('libsqlite3.dylib')
env=dict(os.environ,DYLD_LIBRARY_PATH=str(root))
run([sys.executable,'-c',"import sqlite3; d=sqlite3.connect(':memory:'); print(d.execute('select sqlite_source_id()').fetchone()); assert d.execute(\"select sqlite_compileoption_used('DEBUG')\").fetchone()==(1,)"],env=env)
receipt=json.loads((root/'receipt.json').read_text())
receipt['hashes']={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in root.glob('*') if p.is_file() and p.name!='receipt.json'}
(root/'receipt.json').write_text(json.dumps(receipt,indent=2))
print(json.dumps(dict(sqlite_library=str(root),receipt=str(root/'receipt.json'))),flush=True)
