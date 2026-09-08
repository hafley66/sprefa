"""Shared circuit SQL transport for preserved Take 2 and competitive variants."""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import sqlite3
import time

def emit(**r): print(json.dumps(r),flush=True)
def canonical(rows,prefix): return ''.join(prefix+'\t'+'\t'.join(map(str,r))+'\n' for r in rows)
def digest(text): return hashlib.sha256(text.encode()).hexdigest()

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--fixture',required=True)
    p.add_argument('--db',required=True)
    p.add_argument('--extension',required=True)
    p.add_argument('--batch',action='store_true')
    p.add_argument('--source-views',action='store_true')
    p.add_argument('--frontiers',action='store_true')
    p.add_argument('--lazy',action='store_true')
    args=p.parse_args()
    fixture=json.loads(Path(args.fixture).read_text())
    plans={'aggregate_churn':('join',2,3),'pipeline':('project',1,2),
           'join':('inner',2,2),'self_join':('self_chain',1,2),'chain':('chain',3,2),
           'semijoin':('semi',2,2),'antijoin':('anti',2,2),'reach_cycle':('reach',2,1),
           'distinct':('distinct',1,2),'fanout_fanin':('fanout',1,2),'diamond':('diamond',3,2)}
    if fixture['circuit'] not in plans or (not args.batch and not args.lazy and fixture['circuit']!='aggregate_churn'):
        emit(event='capability',status='unsupported',reason='adapter has no admitted plan for this circuit')
        return
    if Path(args.db).exists(): raise ValueError('existing database')
    start=time.perf_counter()
    db=sqlite3.connect(args.db,isolation_level=None)
    db.enable_load_extension(True);db.load_extension(args.extension);db.enable_load_extension(False)
    if args.batch or args.lazy:db.execute("SELECT take2_control('cache_on')").fetchall()
    if args.source_views:db.execute("SELECT take2_control('source_views_on')").fetchall()
    db.executescript('PRAGMA recursive_triggers=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA cache_size=-8192;')
    for table in ['a','b','c']:
        db.execute(f'CREATE TABLE {table}(id INTEGER PRIMARY KEY,k INTEGER NOT NULL,v INTEGER NOT NULL)')
        db.execute(f'CREATE INDEX {table}_k ON {table}(k)')
        db.execute(f'CREATE INDEX {table}_v ON {table}(v)')
    mode,sides,columns=plans[fixture['circuit']]
    module='take2_lazy' if args.lazy else 'take2_epoch' if args.frontiers else 'take2'
    db.execute(f'CREATE VIRTUAL TABLE result USING {module}({mode})')
    for side,table in enumerate(['a','b','c'][:sides]):
        db.execute("SELECT take2_attach('result',?,?, 'id','k','v')",(table,side)).fetchall()
    emit(event='case-setup',status='ok',setup_ms=(time.perf_counter()-start)*1000,
         algorithm=f'public vtab {mode}',batch=args.batch,source_views=args.source_views,durability='durable SQL WAL/FULL',
         sqlite_version=sqlite3.sqlite_version,extension_sha256=hashlib.sha256(Path(args.extension).read_bytes()).hexdigest())
    if args.frontiers:
        db.executescript('BEGIN;INSERT INTO result(op) VALUES(10);INSERT INTO result(op,id,side) VALUES(12,1,0),(12,1,1);')
        for statement in ['INSERT INTO result(op) VALUES(11)','SELECT * FROM result']:
            try:db.execute(statement).fetchall()
            except sqlite3.DatabaseError:pass
            else:raise AssertionError('held third input must block completion')
        db.execute('INSERT INTO result(op,id,side) VALUES(12,1,2)')
        db.execute('INSERT INTO result(op) VALUES(11)')
        assert db.execute('SELECT * FROM result').fetchall()==[]
        db.execute('ROLLBACK')
        emit(event='frontier-check',status='ok',held_input='c',epoch=1,completion_after_all_inputs_advanced=True,
             contract='finite sequential scalar epochs; explicit input sealing; no partial-order time claim')
    total=0
    for epoch,state in enumerate(fixture['states'],1):
        start=time.perf_counter()
        seal=f'INSERT INTO result(op,id,side) VALUES(12,{epoch},0),(12,{epoch},1),(12,{epoch},2);' if args.frontiers else ''
        db.executescript('BEGIN;'+('INSERT INTO result(op) VALUES(10);' if args.batch else '')+state['mutation_sql']+seal+('INSERT INTO result(op) VALUES(11);' if args.batch else '')+'COMMIT;')
        update_ms=(time.perf_counter()-start)*1000
        start=time.perf_counter()
        output=sorted(map(list,db.execute('SELECT '+','.join(['id','k','v'][:columns])+' FROM result')))
        query_ms=(time.perf_counter()-start)*1000
        assert output==state['expected']['rows']==sorted(map(list,db.execute(fixture['query'])))
        ih=hashlib.sha256()
        for table in ['a','b','c']:
            for row,expected in itertools.zip_longest(db.execute(f'SELECT * FROM {table} ORDER BY id'),sorted(state['inputs'][table])):
                assert row is not None and expected is not None and row==tuple(expected)
                ih.update(canonical([row],table.upper()).encode())
        input_hash=ih.hexdigest();checksum=digest(canonical(output,'S'))
        assert input_hash==state['input_hash'] and checksum==state['expected']['checksum']
        total+=update_ms+query_ms
        emit(event='mutation',status='ok',state=state['name'],exact_input_output_validated=True,
             input_hash=input_hash,checksum=checksum,affected_rows=len(state['writes']),output_rows=len(output),
             update_transaction_ms=update_ms,query_compute_ms=query_ms,update_plus_query_ms=update_ms+query_ms)
    emit(event='case-total',status='ok',update_plus_query_ms=total,final_input_hash=input_hash,final_checksum=checksum,
         disk={'database_bytes':Path(args.db).stat().st_size,'wal_bytes':Path(args.db+'-wal').stat().st_size})
    db.close()
if __name__=='__main__': main()
