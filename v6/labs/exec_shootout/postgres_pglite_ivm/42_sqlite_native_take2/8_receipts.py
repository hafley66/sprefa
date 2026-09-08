"""Reduce immutable shared-runner receipts without rerunning workloads."""
import argparse
import hashlib
import json
import statistics
from pathlib import Path

parser=argparse.ArgumentParser()
parser.add_argument('sweep',type=Path)
parser.add_argument('artifacts',type=Path)
args=parser.parse_args()
rows=[json.loads(line) for line in args.sweep.read_text().splitlines()]
totals=[r for r in rows if r['event']=='case-total']
setups=[r for r in rows if r['event']=='case-setup']
mutations=[r for r in rows if r['event']=='mutation']
assert all(r['status']=='ok' for r in rows)
assert len(totals)==36 and len(mutations)==180
assert all(r['exact_input_output_validated'] for r in mutations)
cells=[]
for key in sorted({(r['rows'],r['batch_size'],r['fanout']) for r in totals}):
    cell=dict(rows=key[0],batch=key[1],fanout=key[2],arms={})
    for arm in ['sqlite-native-take2','sqlite-native-take2-logged']:
        match=lambda r:r['arm']==arm and (r['rows'],r['batch_size'],r['fanout'])==key
        runs=list(filter(match,totals))
        changes=list(filter(match,mutations))
        setup=list(filter(match,setups))
        cell['arms'][arm]=dict(repetitions=len(runs),
            setup_ms_median=statistics.median(r['setup_ms'] for r in setup),
            mutation_ms_sum_per_run=[sum(m['update_transaction_ms'] for m in changes if m['repetition']==r['repetition']) for r in runs],
            query_ms_sum_per_run=[sum(m['query_compute_ms'] for m in changes if m['repetition']==r['repetition']) for r in runs],
            total_ms_median=statistics.median(r['update_plus_query_ms'] for r in runs),
            db_bytes=[r['disk']['database_bytes'] for r in runs],wal_bytes=[r['disk']['wal_bytes'] for r in runs],
            process_peak_rss_bytes=[r['process_peak_rss_platform_units']*(1 if r['rss_units']=='bytes' else 1024) for r in runs])
    cells.append(cell)
logs=[]
for path in sorted(args.artifacts.rglob('stderr.log')):
    lines=path.read_text().splitlines()
    assert len(lines)<=256
    for line in lines: assert set(json.loads(line))=={'callback','depth'}
    logs.append(dict(path=str(path),events=len(lines),bytes=path.stat().st_size,
                     sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
print(json.dumps(dict(sweep_sha256=hashlib.sha256(args.sweep.read_bytes()).hexdigest(),
    case_runs=len(totals),mutation_checks=len(mutations),cells=cells,stderr=logs),indent=2))
