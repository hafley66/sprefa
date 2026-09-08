"""Bounded SQL-oracle tests. Maintenance is exclusively in the extension."""
import os
from pathlib import Path
import random
import sqlite3
import tempfile
import unittest

QUERIES={
 'project':('SELECT k,v*2 FROM a WHERE v>=0',1,2),
 'inner':('SELECT a.k,a.v*b.v FROM a JOIN b ON a.k=b.k',2,2),
 'self_chain':('SELECT x.k,y.v FROM a x JOIN a y ON x.v=y.k',1,2),
 'chain':('SELECT a.k,c.v FROM a JOIN b ON a.v=b.k JOIN c ON b.v=c.k',3,2),
 'semi':('SELECT k,v FROM a WHERE EXISTS(SELECT 1 FROM b WHERE b.k=a.k)',2,2),
 'anti':('SELECT k,v FROM a WHERE NOT EXISTS(SELECT 1 FROM b WHERE b.k=a.k)',2,2),
 'reach':('WITH RECURSIVE r(k) AS(SELECT k FROM b UNION SELECT a.v FROM a JOIN r ON a.k=r.k) SELECT k FROM r',2,1),
 'distinct':('SELECT DISTINCT k,v FROM a',1,2),
 'fanout':('SELECT k,v FROM a WHERE v>=0 UNION ALL SELECT k,v FROM a WHERE v%2=0',1,2),
 'diamond':('SELECT a.k,b.v FROM a JOIN b ON a.v=b.k UNION ALL SELECT a.k,c.v FROM a JOIN c ON a.v=c.k',3,2),
}

class Circuits(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory(dir='/tmp/sprefa-sqlite-competitive')
  self.path=str(Path(self.tmp.name)/'db.sqlite')
  self.db=self.connection()
  for table in 'abc':self.db.execute(f'CREATE TABLE {table}(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER)')
 def connection(self):
  db=sqlite3.connect(self.path,isolation_level=None)
  db.enable_load_extension(True);db.load_extension(os.environ['TAKE2_EXTENSION']);db.enable_load_extension(False)
  db.execute('PRAGMA recursive_triggers=ON')
  db.execute("SELECT take2_control('cache_on')")
  if os.environ.get('TAKE2_SOURCE_VIEWS'):db.execute("SELECT take2_control('source_views_on')")
  return db
 def tearDown(self):self.db.close();self.tmp.cleanup()
 def install(self,mode,args=''):
  self.query,self.sides,self.columns=QUERIES[mode]
  self.db.execute(f'CREATE VIRTUAL TABLE result USING take2({mode}{args})')
  for side,table in enumerate('abc'[:self.sides]):self.db.execute("SELECT take2_attach('result',?,?,'id','k','v')",(table,side)).fetchall()
 def check(self):
  columns=','.join(['id','k','v'][:self.columns])
  order=','.join(str(i+1) for i in range(self.columns))
  self.assertEqual(self.db.execute(f'SELECT {columns} FROM result ORDER BY {order}').fetchall(),self.db.execute(self.query+f' ORDER BY {order}').fetchall())
  self.assertEqual(self.db.execute('SELECT count(*) FROM result_delta').fetchone(),(0,))
 def start(self):self.db.execute('BEGIN');self.db.execute('INSERT INTO result(op) VALUES(10)')
 def flush(self):self.db.execute('INSERT INTO result(op) VALUES(11)');self.check()
 def test_scalar_binding(self):
  self.install('project',",'b0.k BETWEEN -2 AND 2 AND (b0.v IS NULL OR b0.v<>1)','CASE WHEN b0.v IS NULL THEN NULL ELSE abs(b0.v)+b0.k END'")
  self.query='SELECT k,CASE WHEN v IS NULL THEN NULL ELSE abs(v)+k END FROM a WHERE k BETWEEN -2 AND 2 AND (v IS NULL OR v<>1)'
  self.start();self.db.execute('INSERT INTO a VALUES(1,1,NULL),(2,2,-3),(3,2,1),(4,3,1)');self.flush();self.db.execute('COMMIT')
  self.db.close();self.db=self.connection();self.check()
 def test_reject_scalar_plans(self):
  for expression in ['(SELECT v FROM probe)','random()','unknown','sum(v)','?1','v); DELETE FROM a; --']:
   with self.subTest(expression=expression),self.assertRaises(sqlite3.DatabaseError):
    self.db.execute("CREATE VIRTUAL TABLE result USING take2(project,'1','"+expression+"')")
  self.assertEqual(self.db.execute("SELECT name FROM sqlite_schema WHERE name LIKE 'result%'").fetchall(),[])
 def test_reject_projection_domain(self):
  self.install('project',",'1','b0.v/2.0'")
  self.start();self.db.execute('INSERT INTO a VALUES(1,1,3)')
  with self.assertRaises(sqlite3.DatabaseError):self.flush()
  self.db.execute('ROLLBACK');self.check()
 def test_transactional_teardown(self):
  self.install('reach');self.start()
  self.db.execute('INSERT INTO a VALUES(1,1,2),(2,2,1)');self.db.execute('INSERT INTO b VALUES(1,1,0)')
  self.flush();self.db.execute('COMMIT')
  before=self.db.execute("SELECT type,name,sql FROM sqlite_schema ORDER BY type,name").fetchall()
  self.db.execute('BEGIN');self.db.execute('DROP TABLE result');self.db.execute('ROLLBACK');self.check()
  self.assertEqual(self.db.execute("SELECT type,name,sql FROM sqlite_schema ORDER BY type,name").fetchall(),before)
  self.db.execute('DROP TABLE result')
  self.assertEqual(self.db.execute("SELECT name FROM sqlite_schema WHERE name LIKE 'result%' OR name LIKE 'sqlite_autoindex_result%'").fetchall(),[])
  self.db.execute('INSERT INTO a VALUES(3,2,3)')
  self.assertEqual(self.db.execute('SELECT count(*) FROM a').fetchone(),(3,))

def circuit(mode):
 def test(self):
  self.install(mode);rng=random.Random(9201)
  for phase in range(40):
   self.start();self.db.execute('SAVEPOINT inner_batch')
   for step in range(12):
    table='abc'[rng.randrange(self.sides)];identity=rng.randrange(1,15)
    if rng.randrange(4)==0:self.db.execute(f'DELETE FROM {table} WHERE id=?',(identity,))
    else:
     values=[-2,-1,0,1,2,3] if mode=='reach' else [None,-2,-1,0,1,2,3]
     self.db.execute(f'INSERT INTO {table} VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET k=excluded.k,v=excluded.v',(identity,rng.choice(values),rng.choice(values)))
   if phase%5==0:self.db.execute('ROLLBACK TO inner_batch')
   self.flush()
   if phase%7==0:
    self.db.execute('ROLLBACK TO inner_batch')
    with self.assertRaisesRegex(sqlite3.DatabaseError,'flush before reading'):self.check()
    self.flush()
   if phase%6==0:self.db.execute('ROLLBACK')
   elif phase%9==0:
    self.db.execute("SELECT take2_control('fail_sync')")
    with self.assertRaisesRegex(sqlite3.DatabaseError,'injected xSync'):self.db.execute('COMMIT')
   else:self.db.execute('COMMIT')
   self.check()
  for conflict in ['ABORT','FAIL','IGNORE','REPLACE']:
   self.start()
   try:self.db.execute(f'INSERT OR {conflict} INTO a VALUES(50,1,2),(50,2,3),(51,3,1)')
   except sqlite3.IntegrityError:self.assertIn(conflict,['ABORT','FAIL'])
   self.flush();self.db.execute('COMMIT');self.check()
  self.db.close();self.db=self.connection();self.check()
  self.start()
  for table in 'abc'[:self.sides]:self.db.execute(f'DELETE FROM {table}')
  self.flush();self.db.execute('COMMIT');self.check()
 return test
for mode in QUERIES:setattr(Circuits,'test_'+mode,circuit(mode))
if __name__=='__main__':unittest.main(verbosity=2)
