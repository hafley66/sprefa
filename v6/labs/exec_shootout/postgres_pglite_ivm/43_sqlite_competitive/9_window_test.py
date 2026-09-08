"""Event timestamps in k, payload in v, persisted monotone window watermark."""
import os
from pathlib import Path
import random
import sqlite3
import tempfile
import unittest


class Window(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='window-', dir='/tmp/sprefa-sqlite-competitive')
        self.path = Path(self.tmp.name)/'state.db'
        self.db = self.connect()
        self.db.execute('CREATE TABLE a(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER)')
        self.db.execute("CREATE VIRTUAL TABLE result USING take2_counted(window,'3')")
        self.db.execute("SELECT take2_attach('result','a',0,'id','k','v')")
        self.db.execute("SELECT take2_prepare('result')")

    def connect(self):
        db = sqlite3.connect(self.path, isolation_level=None)
        db.enable_load_extension(True)
        db.load_extension(os.environ['TAKE2_EXTENSION'])
        db.execute('PRAGMA recursive_triggers=ON')
        db.execute("SELECT take2_control('cache_on')")
        if os.environ.get('TAKE2_SOURCE_VIEWS') == '1':
            db.execute("SELECT take2_control('source_views_on')")
        return db

    def tearDown(self):
        self.db.close()
        self.tmp.cleanup()

    def check(self):
        self.assertEqual(self.db.execute('SELECT id,k FROM result ORDER BY 1,2').fetchall(), self.db.execute('SELECT k,v FROM a WHERE k BETWEEN (SELECT epoch-2 FROM result_clock) AND (SELECT epoch FROM result_clock) ORDER BY 1,2').fetchall())

    def advance(self, watermark):
        self.db.execute('INSERT INTO result(op,id) VALUES(12,?)', (watermark,))

    def test_expiration_and_lateness(self):
        self.db.execute('INSERT INTO a VALUES(1,-2,5),(2,-1,6),(3,0,NULL),(4,0,NULL)')
        self.check()
        self.advance(2)
        self.check()
        self.assertEqual(self.db.execute('SELECT count(*) FROM a').fetchone(), (4,))
        for stamp in [-1,3]:
            with self.assertRaisesRegex(sqlite3.DatabaseError, 'window'):
                self.db.execute('INSERT INTO a VALUES(5,?,9)', (stamp,))
        self.db.execute('INSERT INTO a VALUES(5,1,9)')
        self.db.execute('DELETE FROM a WHERE id=1')  # expired source deletion is legal
        self.db.execute('UPDATE a SET k=2 WHERE id=2')  # expired record becomes active
        self.check()
        self.advance(5)
        self.check()
        self.assertEqual(self.db.execute('SELECT id,k FROM result').fetchall(), [])

    def test_nested_rollback_and_reopen(self):
        self.db.execute('BEGIN')
        self.db.execute('INSERT INTO a VALUES(1,0,4),(2,-1,5)')
        self.db.execute('SAVEPOINT s')
        self.advance(2)
        self.db.execute('INSERT INTO a VALUES(3,2,6)')
        self.check()
        self.db.execute('SAVEPOINT nested')
        self.advance(4)
        self.db.execute('RELEASE nested')
        self.db.execute('ROLLBACK TO s')
        self.assertEqual(self.db.execute('SELECT epoch FROM result_clock').fetchone(), (0,))
        self.check()
        self.db.execute('RELEASE s')
        self.db.execute('COMMIT')
        self.db.close()
        self.db = self.connect()
        self.check()
        self.advance(1)
        self.check()

    def test_sync_failure_and_invalid_frontier(self):
        self.db.execute('INSERT INTO a VALUES(1,0,2)')
        self.db.execute('BEGIN')
        self.advance(3)
        self.db.execute("SELECT take2_control('fail_sync')")
        with self.assertRaises(sqlite3.DatabaseError):
            self.db.execute('COMMIT')
        self.assertEqual(self.db.execute('SELECT epoch FROM result_clock').fetchone(), (0,))
        self.check()
        for stamp in [-1,0,1.5,1000001]:
            with self.assertRaisesRegex(sqlite3.DatabaseError, 'watermark'):
                self.advance(stamp)
        self.check()

    def test_random_updates_and_advances(self):
        rng = random.Random(933)
        for watermark in range(12):
            self.db.execute('BEGIN')
            if watermark:
                self.advance(watermark)
            for _ in range(7):
                self.db.execute('INSERT INTO a VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET k=excluded.k,v=excluded.v', (rng.randrange(8), rng.randrange(watermark-2,watermark+1), rng.choice([None,0,1,-1])))
            self.check()
            self.db.execute('COMMIT')
        self.db.execute('DELETE FROM a')
        self.check()

    def test_second_writer_and_drop(self):
        self.db.execute('INSERT INTO a VALUES(1,0,2)')
        other = self.connect()
        other.execute('INSERT INTO result(op,id) VALUES(12,3)')
        other.close()
        self.check()
        with self.assertRaisesRegex(sqlite3.DatabaseError, 'window'):
            self.db.execute('INSERT INTO a VALUES(2,0,4)')
        self.db.execute('DROP TABLE result')
        self.assertEqual(self.db.execute("SELECT name FROM sqlite_schema WHERE name LIKE 'result%'").fetchall(), [])

    def test_statement_failure_and_expiration_plan(self):
        self.db.execute('INSERT INTO a VALUES(1,-2,2),(2,0,4)')
        with self.assertRaisesRegex(sqlite3.DatabaseError, 'watermark'):
            self.db.execute('INSERT INTO result(op,id) VALUES(12,2),(12,1)')
        self.assertEqual(self.db.execute('SELECT epoch FROM result_clock').fetchone(), (0,))
        self.check()
        plan = self.db.execute('EXPLAIN QUERY PLAN DELETE FROM result_result WHERE k<2').fetchall()
        self.assertTrue(any('result_result_key' in row[3] and 'k<?' in row[3] for row in plan), plan)
        for conflict in ['ABORT','FAIL','IGNORE','REPLACE']:
            self.db.execute('BEGIN')
            try:
                self.db.execute(f'INSERT OR {conflict} INTO a VALUES(3,0,1),(3,0,2),(4,-1,3)')
            except sqlite3.DatabaseError:
                self.assertIn(conflict, ['ABORT','FAIL'])
            self.check()
            self.db.execute('ROLLBACK')
        self.check()


if __name__ == '__main__':
    unittest.main(verbosity=2)
