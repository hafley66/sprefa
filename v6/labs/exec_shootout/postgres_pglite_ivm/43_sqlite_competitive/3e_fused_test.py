"""Same lazy lifecycle with stats written by the delta statement's SQL trigger."""
import importlib.util
from pathlib import Path
import sqlite3
import unittest

spec = importlib.util.spec_from_file_location('lazy', Path(__file__).with_name('3d_lazy_test.py'))
lazy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lazy)


class Fused(lazy.Lazy):
    module = 'take2_fused'

    def test_transactional_event_counts(self):
        self.install('inner')
        def count(expected):
            self.assertEqual(self.db.execute('SELECT n FROM result_stats').fetchone(), (expected,))
        self.db.execute('BEGIN')
        self.db.execute('INSERT INTO a VALUES(1,1,2),(2,2,3),(3,3,1)')
        count(3)
        self.db.execute('SAVEPOINT s')
        self.db.execute('UPDATE a SET v=v+1')
        count(6)
        self.db.execute('DELETE FROM a WHERE id=1')
        count(7)
        self.db.execute('ROLLBACK TO s')
        count(3)
        self.db.execute('RELEASE s')
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('INSERT OR FAIL INTO a VALUES(4,1,2),(4,1,2)')
        count(4)
        self.db.execute('INSERT OR IGNORE INTO a VALUES(4,1,2),(5,1,2)')
        count(5)
        self.db.execute('INSERT OR REPLACE INTO a VALUES(1,1,2)')
        count(7)
        self.db.execute('UPDATE a SET v=v')
        count(12)
        self.db.execute('DELETE FROM a')
        count(17)
        self.check()
        count(17)
        self.db.execute('ROLLBACK')
        count(0)
        self.check()


class Counted(Fused):
    module = 'take2_counted'

    def test_drop_rollback(self):
        self.install('inner')
        self.db.execute('INSERT INTO a VALUES(1,1,2)')
        self.db.execute('BEGIN')
        self.db.execute('DROP TABLE result')
        self.db.execute('ROLLBACK')
        self.assertEqual(self.db.execute('SELECT n FROM result_stats').fetchone(), (1,))
        self.db.execute('DROP TABLE result')
        self.assertEqual(self.db.execute("SELECT name FROM sqlite_schema WHERE name LIKE 'result%'").fetchall(), [])


if __name__ == '__main__':
    unittest.main(verbosity=2)
