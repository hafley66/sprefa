"""Profile the shared batch1000 fixture, with unchanged SQL oracle checks."""
import importlib.util
import json
from pathlib import Path
import sqlite3
import sys
import time

here=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('oracle',here.parent/'42_sqlite_native_take2/7_shootout.py')
oracle=importlib.util.module_from_spec(spec);spec.loader.exec_module(oracle)
extension,fixture_path,db_path,cache,*rest=sys.argv[1:]
batch=bool(rest and int(rest[0]))
views=bool(len(rest)>1 and int(rest[1]))
fixture=json.loads(Path(fixture_path).read_text())
db=oracle.connection(db_path,extension)
if int(cache):db.execute("SELECT take2_control('cache_on')").fetchall()
db.executescript('PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;CREATE TABLE fact(id INTEGER PRIMARY KEY,group_id INTEGER,amount INTEGER);CREATE TABLE dimension(group_id INTEGER PRIMARY KEY,factor INTEGER);CREATE VIRTUAL TABLE native_result USING take2(join);CREATE VIEW summary AS SELECT id group_id,k n,v s FROM native_result;')
db.execute("SELECT take2_attach('native_result','fact',0,'id','group_id','amount')").fetchall()
db.execute("SELECT take2_attach('native_result','dimension',1,'group_id','group_id','factor')").fetchall()
db.execute('BEGIN')
for rel,sql in [('dimension','INSERT INTO dimension VALUES(?,?)'),('fact','INSERT INTO fact VALUES(?,?,?)')]:db.executemany(sql,fixture['states'][0]['inputs'][rel])
db.execute('COMMIT')
if views:db.execute("SELECT take2_control('source_views_on')").fetchall()
for state in fixture['states'][1:]:
    db.execute("SELECT take2_control('profile_reset')").fetchall()
    start=time.perf_counter_ns()
    db.execute('BEGIN')
    if batch:db.execute('INSERT INTO native_result(op) VALUES(10)')
    db.execute(state['mutation_sql'])
    if batch:db.execute('INSERT INTO native_result(op) VALUES(11)')
    db.execute('COMMIT')
    elapsed=time.perf_counter_ns()-start
    metrics=json.loads(db.execute("SELECT take2_control('profile')").fetchone()[0])
    oracle.verify(db,state)
    print(json.dumps(dict(state=state['name'],cache=bool(int(cache)),batch=batch,source_views=views,elapsed_ns=elapsed,**metrics)),flush=True)
db.close()
