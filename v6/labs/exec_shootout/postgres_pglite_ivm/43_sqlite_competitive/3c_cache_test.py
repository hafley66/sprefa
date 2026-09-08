"""Cached SQL schema invalidation and rollback, with exact SQL output checks."""
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import unittest

class Cache(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory(dir='/tmp/sprefa-sqlite-competitive')
  self.path=str(Path(self.tmp.name)/'cache.db')
  self.db=sqlite3.connect(self.path,isolation_level=None)
  self.db.enable_load_extension(True);self.db.load_extension(os.environ['TAKE2_EXTENSION'])
  self.db.executescript('PRAGMA recursive_triggers=ON;CREATE TABLE a(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);CREATE TABLE b(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);CREATE VIRTUAL TABLE result USING take2(join);')
  self.db.execute("SELECT take2_control('cache_on')")
  for side,table in enumerate('ab'):self.db.execute("SELECT take2_attach('result',?,?,'id','k','v')",(table,side)).fetchall()
  self.db.execute('INSERT INTO a VALUES(1,1,2),(2,2,3)');self.db.execute('INSERT INTO b VALUES(1,1,4),(2,2,5)')
  self.mutate();self.db.execute("SELECT take2_control('profile_reset')")
 def tearDown(self):self.db.close();self.tmp.cleanup()
 def check(self):
  self.assertEqual(self.db.execute('SELECT * FROM result ORDER BY 1').fetchall(),self.db.execute('SELECT a.k,count(*),sum(a.v*b.v) FROM a JOIN b ON a.k=b.k GROUP BY a.k ORDER BY 1').fetchall())
 def mutate(self):self.db.execute('UPDATE a SET v=v+1');self.check()
 def test_index_drop_rollback_and_reprepare(self):
  self.db.execute('BEGIN');self.db.execute('SAVEPOINT schema_change')
  self.db.execute('DROP INDEX result_result_key');self.db.execute('DROP INDEX result_lookup')
  self.mutate();self.db.execute('ROLLBACK TO schema_change');self.check();self.mutate();self.db.execute('COMMIT')
  profile=json.loads(self.db.execute("SELECT take2_control('profile')").fetchone()[0])
  print(json.dumps(dict(case='index_rollback',**profile)))
  self.assertGreater(profile['automatic_reprepares'],0)
  self.db.execute("SELECT take2_control('profile_reset')");self.mutate()
  self.assertEqual(json.loads(self.db.execute("SELECT take2_control('profile')").fetchone()[0])['prepares'],0)
 def test_second_connection_schema_cookie(self):
  other=sqlite3.connect(self.path,isolation_level=None)
  try:other.execute('CREATE INDEX ordinary_index ON a(v)')
  finally:other.close()
  self.mutate()
  profile=json.loads(self.db.execute("SELECT take2_control('profile')").fetchone()[0])
  print(json.dumps(dict(case='second_schema_cookie',**profile)))
  self.assertGreater(profile['automatic_reprepares']+profile['prepares'],0)
  self.db.execute("SELECT take2_control('profile_reset')");self.mutate()
  self.assertEqual(json.loads(self.db.execute("SELECT take2_control('profile')").fetchone()[0])['prepares'],0)
 def test_cached_constraint_failure_and_rollback(self):
  self.db.execute('BEGIN')
  with self.assertRaises(sqlite3.IntegrityError):self.db.execute('INSERT OR FAIL INTO a VALUES(3,1,2),(3,1,3)')
  self.check();self.db.execute('ROLLBACK');self.check();self.mutate()

if __name__=='__main__':unittest.main(verbosity=2)
