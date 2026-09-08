"""Finite scalar epochs: all three inputs seal before output becomes readable."""
import importlib.util
from pathlib import Path
import sqlite3
import unittest

spec=importlib.util.spec_from_file_location('circuits',Path(__file__).with_name('3a_circuit_test.py'))
circuits=importlib.util.module_from_spec(spec);spec.loader.exec_module(circuits)

class Frontiers(circuits.Circuits):
 module='take2_epoch'
 def seal(self,side):
  self.db.execute('INSERT INTO result(op,id,side) SELECT 12,epoch,? FROM result_clock',(side,))
 def flush(self):
  for side in range(3):self.seal(side)
  super().flush()
 def test_held_input_and_late_write(self):
  self.install('project');self.start();self.db.execute('INSERT INTO a VALUES(1,1,2)')
  self.seal(0);self.seal(1)
  with self.assertRaisesRegex(sqlite3.DatabaseError,'all input frontiers'):self.db.execute('INSERT INTO result(op) VALUES(11)')
  with self.assertRaisesRegex(sqlite3.DatabaseError,'flush before reading'):self.check()
  with self.assertRaisesRegex(sqlite3.DatabaseError,'sealed input'):self.db.execute('INSERT INTO a VALUES(2,2,3)')
  with self.assertRaisesRegex(sqlite3.DatabaseError,'strictly advance'):self.seal(0)
  self.seal(2);super().flush();self.db.execute('COMMIT');self.check()
  self.assertEqual(self.db.execute('SELECT side,t FROM result_frontier ORDER BY side').fetchall(),[(0,1),(1,1),(2,1)])
  self.start();self.seal(0)
  with self.assertRaisesRegex(sqlite3.DatabaseError,'explicit flush required'):self.db.execute('COMMIT')
  self.assertFalse(self.db.in_transaction);self.check()
  self.assertEqual(self.db.execute('SELECT epoch FROM result_clock').fetchone(),(1,))
 def test_epoch_domain(self):
  self.install('project');self.start()
  for epoch,side in [(0,0),(2,0),(1,3),(1,-1),(None,0)]:
   with self.subTest(epoch=epoch,side=side),self.assertRaises(sqlite3.DatabaseError):
    self.db.execute('INSERT INTO result(op,id,side) VALUES(12,?,?)',(epoch,side))
  self.db.execute('ROLLBACK');self.check()
 def test_second_writer_and_absent_module(self):
  self.install('project');self.start();self.db.execute('INSERT INTO a VALUES(1,1,2)');self.flush();self.db.execute('COMMIT')
  absent=sqlite3.connect(self.path,isolation_level=None)
  try:
   with self.assertRaises(sqlite3.DatabaseError):absent.execute('UPDATE a SET v=3')
  finally:absent.close()
  other=self.connection()
  try:
   with self.assertRaisesRegex(sqlite3.DatabaseError,'open batch'):other.execute('UPDATE a SET v=3')
   other.execute('BEGIN');other.execute('INSERT INTO result(op) VALUES(10)');other.execute('UPDATE a SET v=3')
   other.execute('INSERT INTO result(op,id,side) VALUES(12,2,0),(12,2,1),(12,2,2)')
   other.execute('INSERT INTO result(op) VALUES(11)')
   self.assertEqual(other.execute('SELECT id,k FROM result').fetchall(),[(1,6)])
   other.execute('COMMIT')
  finally:other.close()
  self.check();self.assertEqual(self.db.execute('SELECT epoch FROM result_clock').fetchone(),(2,))

if __name__=='__main__':unittest.main(verbosity=2)
