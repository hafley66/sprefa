"""Public-ABI read/xSync flush probe, exact completed-statement visibility."""
import importlib.util
from pathlib import Path
import random
import sqlite3
import unittest

spec=importlib.util.spec_from_file_location('circuits',Path(__file__).with_name('3a_circuit_test.py'))
c=importlib.util.module_from_spec(spec);spec.loader.exec_module(c)
class Lazy(unittest.TestCase):
 module='take2_lazy'
 setUp=c.Circuits.setUp
 tearDown=c.Circuits.tearDown
 connection=c.Circuits.connection
 install=c.Circuits.install
 check=c.Circuits.check
 def test_first_multirow_index_integrity(self):
  self.install('self_chain')
  self.db.execute('INSERT INTO a VALUES(1,1,2),(2,2,3),(3,3,1)')
  self.check()
  self.assertEqual(self.db.execute('PRAGMA integrity_check').fetchall(),[('ok',)])
  self.db.close();self.db=self.connection()
  self.db.execute('DELETE FROM a');self.check()
  self.assertEqual(self.db.execute('PRAGMA integrity_check').fetchall(),[('ok',)])
 def test_layout_setup_misuse_and_rollback(self):
  self.db.execute("SELECT take2_control('source_views_on')")
  self.db.execute('CREATE VIRTUAL TABLE result USING take2_lazy(self_chain)')
  self.db.execute("SELECT take2_attach('result','a',0,'id','k','v')")
  with self.assertRaisesRegex(sqlite3.DatabaseError,'take2_prepare'):
   self.db.execute('INSERT INTO a VALUES(1,1,2),(2,2,3)')
  self.assertEqual(self.db.execute('SELECT * FROM a').fetchall(),[])
  with self.assertRaisesRegex(sqlite3.DatabaseError,'direct take2_prepare'):
   self.db.execute('INSERT INTO result(op) VALUES(13)')
  self.db.execute("CREATE TRIGGER forbidden BEFORE INSERT ON a BEGIN SELECT take2_prepare('result'); END")
  with self.assertRaisesRegex(sqlite3.DatabaseError,'unsafe'):
   self.db.execute('INSERT INTO a VALUES(1,1,2)')
  self.db.execute('DROP TRIGGER forbidden')
  self.db.execute('BEGIN');self.db.execute("SELECT take2_prepare('result')");self.db.execute('ROLLBACK')
  with self.assertRaisesRegex(sqlite3.DatabaseError,'take2_prepare'):
   self.db.execute('INSERT INTO a VALUES(1,1,2)')
  self.db.execute("SELECT take2_prepare('result')")
  self.db.execute('INSERT INTO a VALUES(1,1,2),(2,2,3)')
  self.assertEqual(self.db.execute('SELECT id,k FROM result').fetchall(),[(1,3)])
  self.assertEqual(self.db.execute('PRAGMA integrity_check').fetchall(),[('ok',)])
 def test_sync_failure(self):
  self.install('inner');self.db.execute('BEGIN')
  self.db.execute('INSERT INTO a VALUES(1,1,2)');self.db.execute('INSERT INTO b VALUES(1,1,3)')
  self.db.execute("SELECT take2_control('fail_sync')")
  with self.assertRaises(sqlite3.DatabaseError):self.db.execute('COMMIT')
  self.assertFalse(self.db.in_transaction);self.check()
  self.assertEqual(self.db.execute('SELECT * FROM a').fetchall(),[])
 def test_autocommit_conflicts(self):
  self.install('inner');self.db.execute('INSERT INTO b VALUES(1,1,3)')
  for conflict in ['ABORT','FAIL','IGNORE','REPLACE']:
   self.db.execute('DELETE FROM a')
   try:self.db.execute(f'INSERT OR {conflict} INTO a VALUES(1,1,2),(1,1,4),(2,1,5)')
   except sqlite3.IntegrityError:self.assertIn(conflict,['ABORT','FAIL'])
   self.assertFalse(self.db.in_transaction);self.check()
 def test_second_writer(self):
  self.install('inner');self.db.execute('INSERT INTO a VALUES(1,1,2)');self.db.execute('INSERT INTO b VALUES(1,1,3)')
  other=self.connection()
  try:other.execute('UPDATE a SET v=4')
  finally:other.close()
  self.check()
  absent=sqlite3.connect(self.path,isolation_level=None)
  try:
   with self.assertRaises(sqlite3.DatabaseError):absent.execute('UPDATE a SET v=5')
  finally:absent.close()
  self.check()
 def test_failed_read_then_retry(self):
  self.install('inner');self.db.execute('BEGIN')
  self.db.execute('INSERT INTO a VALUES(1,1,2)');self.db.execute('INSERT INTO b VALUES(1,1,3)')
  self.db.create_function('fail',0,lambda:1/0)
  with self.assertRaises(sqlite3.DatabaseError):self.db.execute('SELECT fail() FROM result').fetchall()
  self.check();self.db.execute('ROLLBACK');self.check()
 def test_read_in_source_trigger_then_abort(self):
  self.install('inner')
  self.db.execute('INSERT INTO b VALUES(1,1,3)')
  self.db.executescript("CREATE TRIGGER read_prefix AFTER INSERT ON a BEGIN SELECT * FROM result; SELECT CASE WHEN NEW.id=2 THEN RAISE(ABORT,'after prefix read') END; END;")
  with self.assertRaises(sqlite3.IntegrityError):self.db.execute('INSERT INTO a VALUES(1,1,2),(2,1,4)')
  self.check();self.assertEqual(self.db.execute('SELECT * FROM a').fetchall(),[])

def family(mode):
 def test(self):
  self.install(mode);rng=random.Random(885)
  for phase in range(18):
   explicit=phase%3!=0
   if explicit:self.db.execute('BEGIN');self.db.execute('SAVEPOINT phase')
   for i in range(8):
    table='abc'[rng.randrange(self.sides)];identity=rng.randrange(1,10)
    values=[-2,0,1,2,3] if mode=='reach' else [None,-2,0,1,2,3]
    self.db.execute(f'INSERT INTO {table} VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET k=excluded.k,v=excluded.v',(identity,rng.choice(values),rng.choice(values)))
    self.check()
   if explicit:
    if phase%4==0:self.db.execute('ROLLBACK TO phase');self.check()
    self.db.execute('COMMIT');self.check()
  for conflict in ['ABORT','FAIL','IGNORE','REPLACE']:
   self.db.execute('BEGIN')
   try:self.db.execute(f'INSERT OR {conflict} INTO a VALUES(50,1,2),(50,2,3),(51,3,1)')
   except sqlite3.IntegrityError:self.assertIn(conflict,['ABORT','FAIL'])
   self.check();self.db.execute('COMMIT');self.check()
  self.db.execute('BEGIN')
  for table in 'abc'[:self.sides]:self.db.execute(f'DELETE FROM {table}')
  self.db.execute('COMMIT');self.check()
  self.db.close();self.db=self.connection();self.check()
 return test
for mode in c.QUERIES:setattr(Lazy,'test_'+mode,family(mode))
if __name__=='__main__':unittest.main(verbosity=2)
