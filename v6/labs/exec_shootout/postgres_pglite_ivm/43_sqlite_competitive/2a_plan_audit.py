"""Exact plan audit for key lookup, validation and cleanup SQL. No maintenance."""
import hashlib
import json
from pathlib import Path
import sqlite3
import sys

extension,path=sys.argv[1:]
if Path(path).exists():raise ValueError('fresh task-owned database required')
db=sqlite3.connect(path,isolation_level=None,cached_statements=0)
db.enable_load_extension(True);db.load_extension(extension);db.enable_load_extension(False)
db.executescript('PRAGMA recursive_triggers=ON;CREATE TABLE a(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);CREATE TABLE b(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);CREATE VIRTUAL TABLE result USING take2(join);')
for side,table in enumerate('ab'):db.execute("SELECT take2_attach('result',?,?,'id','k','v')",(table,side)).fetchall()
db.execute("SELECT take2_control('cache_on')")
for table in 'ab':db.execute(f'WITH RECURSIVE x(i) AS(VALUES(1) UNION ALL SELECT i+1 FROM x WHERE i<1000) INSERT INTO {table} SELECT i,i%100,i%7 FROM x')
db.execute('ANALYZE')
queries={
 'text_primary_key':'SELECT n FROM result_result WHERE key=?1',
 'invariant_key':'SELECT count(*) FROM result_result WHERE k IS ?1 AND (n<0 OR nn<0 OR nn>n OR typeof(n)<>\'integer\' OR typeof(s)<>\'integer\' OR typeof(nn)<>\'integer\' OR (n=0 AND (s<>0 OR nn<>0)))',
 'zero_cleanup':'DELETE FROM result_result WHERE k IS ?1 AND n=0',
 'state_join_key':'SELECT k,v FROM result_state WHERE side=1 AND k=?1',
}
def plans():return {name:[row[3] for row in db.execute('EXPLAIN QUERY PLAN '+sql,(1,))] for name,sql in queries.items()}
before=plans()
assert 'sqlite_autoindex_result_result_1' in ' '.join(before['text_primary_key'])
for key in ['invariant_key','zero_cleanup']:assert 'SEARCH result_result USING INDEX result_result_key' in ' '.join(before[key])
assert 'result_lookup' in ' '.join(before['state_join_key'])
db.execute('BEGIN');db.execute('DROP INDEX result_result_key')
without=plans()
for key in ['invariant_key','zero_cleanup']:assert 'SCAN result_result' in ' '.join(without[key])
db.execute('ROLLBACK');assert plans()==before
db.execute("SELECT take2_control('source_views_on')")
db.execute('BEGIN');db.execute('INSERT INTO result(op) VALUES(10)');db.execute('INSERT INTO result(op) VALUES(11)');db.execute('COMMIT')
source=[r[3] for r in db.execute('EXPLAIN QUERY PLAN SELECT k,v FROM result_live_1 WHERE k=?1',(1,))]
assert 'result_live_key_1' in ' '.join(source)
print(json.dumps(dict(sqlite_version=sqlite3.sqlite_version,extension_sha256=hashlib.sha256(Path(extension).read_bytes()).hexdigest(),queries=queries,indexed=before,without_result_k_index=without,rollback_restores_plans=True,source_view_key=source)))
db.close()
