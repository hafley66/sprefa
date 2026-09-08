"""DRed cyclic retraction with scalar predicates on edges and roots."""
import os
from pathlib import Path
import random
import sqlite3
import tempfile
import unittest


class FilteredReach(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='filtered-reach-', dir='/tmp/sprefa-sqlite-competitive')
        self.path = Path(self.tmp.name)/'state.db'
        self.db = self.connect()
        self.db.executescript('CREATE TABLE a(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);CREATE TABLE b(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);')
        self.db.execute("CREATE VIRTUAL TABLE result USING take2_counted(reach,'b0.k<>b0.v AND b0.v<>99','b0.v>=0')")
        for side, source in enumerate('ab'):
            self.db.execute("SELECT take2_attach('result',?,?,'id','k','v')", (source,side))
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
        query = 'WITH RECURSIVE r(k) AS (SELECT k FROM b WHERE v>=0 UNION SELECT a.v FROM a JOIN r ON a.k=r.k WHERE a.k<>a.v AND a.v<>99) SELECT k FROM r ORDER BY k'
        self.assertEqual(self.db.execute('SELECT id FROM result ORDER BY id').fetchall(), self.db.execute(query).fetchall())

    def test_cycle_root_predicate_retraction(self):
        self.db.execute('BEGIN')
        self.db.execute('INSERT INTO a VALUES(1,1,2),(2,2,3),(3,3,2),(4,3,99),(5,4,3),(6,1,1)')
        self.db.execute('INSERT INTO b VALUES(1,1,0),(2,4,-1)')
        self.check()
        self.db.execute('UPDATE b SET v=-1 WHERE id=1')
        self.check()
        self.assertEqual(self.db.execute('SELECT id FROM result').fetchall(), [])
        self.db.execute('SAVEPOINT s')
        self.db.execute('UPDATE b SET v=1 WHERE id=2')
        self.check()
        self.db.execute('DELETE FROM a WHERE id=5')
        self.check()
        self.db.execute('ROLLBACK TO s')
        self.check()
        self.db.execute('RELEASE s')
        self.db.execute('COMMIT')
        self.db.close()
        self.db = self.connect()
        self.check()

    def test_random_predicate_crossings(self):
        rng = random.Random(1077)
        for phase in range(15):
            self.db.execute('BEGIN')
            for _ in range(12):
                table = rng.choice('ab')
                self.db.execute(f'INSERT INTO {table} VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET k=excluded.k,v=excluded.v', (rng.randrange(12),rng.randrange(-2,6),rng.choice([-2,-1,0,1,2,3,4,5,99])))
            self.check()
            if phase % 3 == 0:
                self.db.execute('ROLLBACK')
            else:
                self.db.execute('COMMIT')
            self.check()
        self.db.execute('DELETE FROM b')
        self.check()
        self.assertEqual(self.db.execute('SELECT id FROM result').fetchall(), [])

    def test_invalid_recursive_predicate_rejected(self):
        for predicate in ['random()>0', '(SELECT k FROM b)', 'b0.unknown>0', '?1']:
            with self.assertRaisesRegex(sqlite3.DatabaseError, 'scalar'):
                self.db.execute(f"CREATE VIRTUAL TABLE invalid USING take2_counted(reach,'{predicate}','1')")
        self.assertEqual(self.db.execute("SELECT name FROM sqlite_schema WHERE name LIKE 'invalid%'").fetchall(), [])


if __name__ == '__main__':
    unittest.main(verbosity=2)
