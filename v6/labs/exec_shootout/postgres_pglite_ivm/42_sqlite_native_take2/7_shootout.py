"""Bounded shared-fixture SQL transport. All maintained state is extension-owned."""
import argparse
import hashlib
import itertools
import json
import resource
import sqlite3
import sys
import time
from pathlib import Path

QUERY = 'SELECT f.group_id,count(*),sum(f.amount*d.factor) FROM fact f JOIN dimension d ON f.group_id=d.group_id GROUP BY f.group_id ORDER BY 1'
SELECT = 'SELECT group_id,n,s FROM summary ORDER BY group_id'

def emit(**row):
    print(json.dumps(row),flush=True)

def connection(path, extension):
    db=sqlite3.connect(path,isolation_level=None)
    db.enable_load_extension(True)
    db.load_extension(extension)
    db.enable_load_extension(False)
    db.execute('PRAGMA recursive_triggers=ON')
    db.execute('PRAGMA cache_size=-8192')
    return db

def verify(db,state):
    digest=hashlib.sha256()
    first=True
    for rel,prefix in [('dimension','D'),('fact','F')]:
        for actual,expected in itertools.zip_longest(db.execute(f'SELECT * FROM {rel} ORDER BY 1'),state['inputs'][rel]):
            assert actual is not None and expected is not None and actual==tuple(expected), (rel,'input mismatch')
            digest.update(('' if first else '\n').encode())
            digest.update((prefix+'\t'+'\t'.join(map(str,actual))).encode())
            first=False
    input_hash=digest.hexdigest()
    assert input_hash==state['input_hash'],'shared input hash mismatch'
    summary=db.execute(SELECT+' LIMIT 257').fetchall()
    oracle=db.execute(QUERY+' LIMIT 257').fetchall()
    assert len(summary)<=256 and summary==oracle,'fresh SQL oracle mismatch'
    assert [tuple(map(int,row)) for row in state['expected']['summary']]==summary,'shared output oracle mismatch'
    canonical='\n'.join('S\t'+'\t'.join(map(str,row)) for row in summary)
    checksum=hashlib.sha256(canonical.encode()).hexdigest()
    assert checksum==state['expected']['checksum'],'shared output checksum mismatch'
    return summary,checksum,input_hash,len(canonical.encode())

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--fixture',required=True)
    parser.add_argument('--db',required=True)
    parser.add_argument('--extension',required=True)
    parser.add_argument('--logged',type=int,default=0)
    args=parser.parse_args()
    fixture_path=Path(args.fixture)
    if fixture_path.stat().st_size>64*1024*1024: raise ValueError('fixture exceeds 64 MiB transport cap')
    fixture=json.loads(fixture_path.read_text())
    if len(fixture['states'])>256 or any(len(s['inputs']['fact'])>13000 for s in fixture['states']):
        raise ValueError('fixture exceeds bounded 256-state/13000-fact transport domain')
    if Path(args.db).exists(): raise ValueError('refusing to overwrite database')
    started=time.perf_counter()
    db=connection(args.db,args.extension)
    db.execute('PRAGMA journal_mode=WAL')
    db.execute('PRAGMA synchronous=FULL')
    db.executescript('CREATE TABLE fact(id INTEGER PRIMARY KEY,group_id INTEGER NOT NULL,amount INTEGER NOT NULL); CREATE TABLE dimension(group_id INTEGER PRIMARY KEY,factor INTEGER NOT NULL); CREATE INDEX fact_group_idx ON fact(group_id); CREATE VIRTUAL TABLE native_result USING take2(join); CREATE VIEW summary AS SELECT id AS group_id,k AS n,v AS s FROM native_result;')
    db.execute("SELECT take2_attach('native_result','fact',0,'id','group_id','amount')").fetchall()
    db.execute("SELECT take2_attach('native_result','dimension',1,'group_id','group_id','factor')").fetchall()
    db.execute('BEGIN IMMEDIATE')
    initial=fixture['states'][0]['inputs']
    db.executemany('INSERT INTO dimension VALUES(?,?)',initial['dimension'])
    db.executemany('INSERT INTO fact VALUES(?,?,?)',initial['fact'])
    db.execute('COMMIT')
    setup_ms=(time.perf_counter()-started)*1000
    db.execute("SELECT take2_control('trace')").fetchall()
    if args.logged: db.execute("SELECT take2_control('log_on')").fetchall()
    emit(event='case-setup',status='ok',setup_ms=setup_ms,
         runtime='SQLite '+sqlite3.sqlite_version,algorithm='extension xUpdate signed join COUNT/SUM; synchronous per event',
         durability='durable SQL: WAL/synchronous=FULL',initialization='empty install followed by incremental source loading',
         extension_sha256=hashlib.sha256(Path(args.extension).read_bytes()).hexdigest(),
         memory_scope='Python transport + SQLite + extension; 8 MiB SQLite page cache, host-pressure/deadline guarded; no total RSS cap',
         logging={'enabled':bool(args.logged),'event_limit':256,'sink':'stderr','payloads':False},
         counters='persistent transaction-safe source-event count; always enabled')
    total=0
    previous=set()
    for state in fixture['states']:
        started=time.perf_counter()
        affected=0
        if state['name']!='initial':
            db.execute('BEGIN IMMEDIATE')
            try:
                db.execute(state['mutation_sql'])
                affected=db.execute('SELECT changes()').fetchone()[0]
                db.execute('COMMIT')
            except Exception:
                if db.in_transaction: db.execute('ROLLBACK')
                raise
        update_ms=(time.perf_counter()-started)*1000 if state['name']!='initial' else 0
        started=time.perf_counter()
        db.execute('CREATE TEMP TABLE measured_output AS '+SELECT)
        count=db.execute('SELECT count(*) FROM measured_output').fetchone()[0]
        query_ms=(time.perf_counter()-started)*1000
        summary,checksum,input_hash,output_bytes=verify(db,state)
        assert affected==state['expected_affected_rows'] and count==len(summary)
        db.execute('DROP TABLE measured_output')
        current=set(summary)
        if state['name']!='initial': total+=update_ms+query_ms
        emit(event='mutation',status='ok',state=state['name'],affected_rows=affected,
             join_affected_rows=state['join_affected_rows'],update_transaction_ms=update_ms,
             query_compute_ms=query_ms,update_plus_query_ms=update_ms+query_ms,
             checksum=checksum,input_hash=input_hash,output_rows=len(summary),output_bytes=output_bytes,
             materialized_count=count,summary=summary,output_insertions=len(current-previous),
             output_retractions=len(previous-current),exact_input_output_validated=True,
             source_events=db.execute('SELECT n FROM native_result_stats').fetchone()[0])
        previous=current
    disk={name:Path(args.db+suffix).stat().st_size if Path(args.db+suffix).exists() else 0
          for name,suffix in [('main_bytes',''),('wal_bytes','-wal'),('shm_bytes','-shm')]}
    db.close()
    db=connection(args.db,args.extension)
    verify(db,fixture['states'][-1])
    db.close()
    emit(event='case-total',status='ok',update_plus_query_ms=total,final_checksum=checksum,
         final_input_hash=input_hash,fresh_reopen_validated=True,
         disk={**disk,'database_bytes':Path(args.db).stat().st_size,'temp_bytes':None,'temp_scope':'temporary files not sampled'},
         process_peak_rss_platform_units=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
         rss_units='bytes' if sys.platform=='darwin' else 'KiB')

if __name__=='__main__': main()
