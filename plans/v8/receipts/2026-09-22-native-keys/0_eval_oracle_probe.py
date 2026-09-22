"""Run each v8 eval oracle with its own five-second diagnostic budget."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

root = Path(__file__).resolve().parents[4]
receipt = Path(__file__).resolve().parent
binary = root / 'target/debug/dl8'
library = (root / 'sqlite_ivm/target/extension/release/libsqlite_ivm.dylib').resolve()
env = dict(os.environ, DL8_ENGINE='sqlite', RUST_LOG='off', SQLITE_IVM_LIB=str(library))
rows = []
for source in sorted((root / 'oracle/eval').glob('*.json')):
    case = json.loads(source.read_text())
    if 'expected' not in case:
        continue
    program = receipt / ('input-' + source.name)
    program.write_text(json.dumps(case['program']))
    command = [str(binary), 'eval', str(program)]
    started = time.monotonic()
    row = dict(case=source.name, command=command, timeout_seconds=5)
    try:
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=5)
        output = receipt / ('output-' + source.name)
        output.write_bytes(result.stdout)
        (receipt / ('stderr-' + source.stem + '.log')).write_bytes(result.stderr)
        got = json.loads(result.stdout)
        want = case['expected']
        same = (got['closure'] == want['closure'] and got['diagnostics'] == want['diagnostics']
                and result.returncode == (1 if want.get('diagnostics') else 0))
        row.update(status='equal' if same else 'differs', exit_code=result.returncode,
                   output=output.name, output_sha256=hashlib.sha256(result.stdout).hexdigest())
    except subprocess.TimeoutExpired:
        row['status'] = 'timeout'
    except (ValueError, KeyError) as error:
        row.update(status='invalid-output', error=str(error))
    row['seconds'] = round(time.monotonic() - started, 6)
    rows.append(row)
    print(row['case'], row['status'], row['seconds'], flush=True)
    (receipt / 'eval-oracles.json').write_text(json.dumps(dict(
        compiler_head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
        extension_sha256=hashlib.sha256(library.read_bytes()).hexdigest(),
        note='Working-tree CTE fix; diagnostic runs, not paired performance benchmarks.', cases=rows),indent=2)+'\n')
