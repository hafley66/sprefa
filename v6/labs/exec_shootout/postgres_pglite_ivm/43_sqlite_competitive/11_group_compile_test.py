"""Compiler-to-delta grouped COUNT/SUM oracles on general joins."""
import importlib.util
from pathlib import Path
import subprocess
import sqlite3
import unittest

spec = importlib.util.spec_from_file_location('compiled', Path(__file__).with_name('7_compile_test.py'))
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)
c.QUERIES = [
    'SELECT k%2,COUNT(*),SUM(v) FROM a WHERE v>=0 GROUP BY k%2',
    'SELECT x.k,COUNT(*),SUM(x.v*y.v) FROM a x JOIN b y ON x.k<y.k GROUP BY x.k',
    'SELECT x.k+1,COUNT(*),SUM(z.v) FROM a x JOIN a y ON x.v=y.k JOIN a z ON y.v=z.k GROUP BY x.k+1',
    'SELECT k,COUNT(*),SUM(v) FROM a GROUP BY k',
]


class Grouped(c.CompilerOracle):
    def test_sum_expression_type_guard(self):
        db = self.connect(':memory:')
        db.execute('CREATE TABLE a(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER)')
        db.execute(c.compile_sql('SELECT k,COUNT(*),SUM(v/2.0) FROM a GROUP BY k')['create_sql'])
        db.execute("SELECT take2_attach('compiled_result','a',0,'id','k','v')")
        db.execute("SELECT take2_prepare('compiled_result')")
        with self.assertRaisesRegex(sqlite3.DatabaseError, 'SUM argument'):
            db.execute('INSERT INTO a VALUES(1,1,2)')
        self.assertEqual(db.execute('SELECT * FROM a').fetchall(), [])
        db.close()

    def test_unsupported_aggregate_shapes(self):
        for query in ['SELECT k,COUNT(v),SUM(v) FROM a GROUP BY k',
                      'SELECT k,COUNT(*),MAX(v) FROM a GROUP BY k',
                      'SELECT k,COUNT(*),SUM(DISTINCT v) FROM a GROUP BY k',
                      'SELECT k,COUNT(*),SUM(v) FROM a GROUP BY v',
                      'SELECT k,COUNT(*),SUM(v) FROM a GROUP BY k HAVING COUNT(*)>1']:
            process = subprocess.run([c.COMPILER], input=query, text=True, capture_output=True)
            self.assertEqual(process.returncode, 2, query)


if __name__ == '__main__':
    unittest.main(verbosity=2)
