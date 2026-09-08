"""Explicit-batch lifecycle, cross-term and conflict contracts."""
import importlib.util
import os
from pathlib import Path
import random
import sqlite3
import tempfile
import unittest

here=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('families',here.parent/'42_sqlite_native_take2/6_semantic_test.py')
families=importlib.util.module_from_spec(spec);spec.loader.exec_module(families)

class Batch(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(dir='/tmp/sprefa-sqlite-competitive')
        self.db=sqlite3.connect(str(Path(self.tmp.name)/'test.db'),isolation_level=None)
        self.db.enable_load_extension(True);self.db.load_extension(os.environ['TAKE2_EXTENSION'])
        self.db.executescript('PRAGMA recursive_triggers=ON;CREATE TABLE a(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);CREATE TABLE b(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);CREATE TABLE c(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);')
        self.db.execute("SELECT take2_control('cache_on')").fetchall()
        if os.environ.get('TAKE2_SOURCE_VIEWS'):self.db.execute("SELECT take2_control('source_views_on')").fetchall()
    def tearDown(self):self.db.close();self.tmp.cleanup()
    def install(self,mode):
        self.mode=mode
        self.db.execute(f'CREATE VIRTUAL TABLE result USING take2({mode})')
        self.sources=['a','b','c'][:{'join':2,'multi':3}.get(mode,1)]
        for side,table in enumerate(self.sources):self.db.execute("SELECT take2_attach('result',?,?, 'id','k','v')",(table,side)).fetchall()
    def check(self):
        self.assertEqual(self.db.execute('SELECT * FROM result ORDER BY 1,2,3').fetchall(),self.db.execute(families.QUERIES[self.mode]+' ORDER BY 1,2,3').fetchall())
    def begin(self):self.db.execute('INSERT INTO result(op) VALUES(10)')
    def flush(self):self.db.execute('INSERT INTO result(op) VALUES(11)')
    def unreadable(self):
        with self.assertRaisesRegex(sqlite3.DatabaseError,'flush before reading'):self.db.execute('SELECT * FROM result').fetchall()
    def test_misuse(self):
        self.install('join')
        with self.assertRaisesRegex(sqlite3.DatabaseError,'explicit transaction'):self.begin()
        self.db.execute('BEGIN');self.begin()
        with self.assertRaisesRegex(sqlite3.DatabaseError,'already open'):self.begin()
        self.db.execute('INSERT INTO a VALUES(1,1,2)');self.db.execute('INSERT INTO b VALUES(1,1,3)')
        self.unreadable()
        with self.assertRaisesRegex(sqlite3.DatabaseError,'explicit flush required'):self.db.execute('COMMIT')
        self.assertFalse(self.db.in_transaction);self.check()
        self.assertEqual(self.db.execute('SELECT * FROM a').fetchall(),[])
        self.db.execute('BEGIN')
        with self.assertRaisesRegex(sqlite3.DatabaseError,'no open batch'):self.flush()
        self.db.execute('ROLLBACK')
    def test_savepoints_and_failed_flush(self):
        self.install('multi');self.db.execute('BEGIN');self.begin()
        self.db.execute('INSERT INTO a VALUES(1,1,1000000)');self.db.execute('INSERT INTO b VALUES(1,1,1000000)')
        self.db.execute('SAVEPOINT before_overflow')
        self.db.execute('WITH RECURSIVE x(i) AS(VALUES(1) UNION ALL SELECT i+1 FROM x WHERE i<10) INSERT INTO c SELECT i,1,1000000 FROM x')
        with self.assertRaises(sqlite3.DatabaseError):self.flush()
        self.unreadable()
        self.db.execute('ROLLBACK TO before_overflow')
        self.db.execute('INSERT INTO c VALUES(1,1,1)');self.flush();self.check()
        self.db.execute('ROLLBACK TO before_overflow');self.unreadable()
        self.flush();self.check();self.db.execute('COMMIT');self.check()
    def test_source_view_writer_configuration(self):
        self.install('join')
        self.db.execute("SELECT take2_control('source_views_on')")
        self.db.execute('BEGIN');self.begin()
        self.db.execute('INSERT INTO a VALUES(1,1,2)');self.db.execute('INSERT INTO b VALUES(1,1,3)')
        self.flush();self.db.execute('COMMIT');self.check()
        self.assertEqual(self.db.execute('SELECT count(*) FROM result_state').fetchone(),(0,))
        with self.assertRaisesRegex(sqlite3.DatabaseError,'open batch'):self.db.execute('UPDATE a SET v=4')
        other=sqlite3.connect(str(Path(self.tmp.name)/'test.db'),isolation_level=None)
        other.enable_load_extension(True);other.load_extension(os.environ['TAKE2_EXTENSION'])
        other.execute('PRAGMA recursive_triggers=ON');other.execute('BEGIN')
        other.execute('INSERT INTO result(op) VALUES(10)')
        with self.assertRaisesRegex(sqlite3.DatabaseError,'source_views_on'):other.execute('UPDATE a SET v=4')
        other.execute('ROLLBACK');other.close();self.check()

def family(mode):
    def test(self):
        self.install(mode)
        rng=random.Random(771)
        for phase in range(12):
            self.db.execute('BEGIN');self.begin()
            self.db.execute('SAVEPOINT within_batch')
            for step in range(25):
                table=self.sources[step%len(self.sources)]
                row=(rng.randrange(1,10),rng.choice([None,0,1,2]),rng.choice([None,-2,0,3]))
                self.db.execute(f'INSERT INTO {table} VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET k=excluded.k,v=excluded.v',row)
            self.unreadable()
            if phase%3==0:self.db.execute('ROLLBACK TO within_batch')
            self.flush();self.check()
            if phase%4==0:
                self.db.execute("SELECT take2_control('fail_sync')").fetchall()
                with self.assertRaisesRegex(sqlite3.DatabaseError,'injected xSync'):self.db.execute('COMMIT')
            else:self.db.execute('COMMIT')
            self.check()
        for conflict in ['ABORT','FAIL','IGNORE','REPLACE']:
            self.db.execute('BEGIN');self.begin()
            statement=f'INSERT OR {conflict} INTO a VALUES(20,1,2),(20,1,3),(21,2,5)'
            if conflict in ['ABORT','FAIL']:
                with self.assertRaises(sqlite3.IntegrityError):self.db.execute(statement)
            else:self.db.execute(statement)
            self.flush();self.check();self.db.execute('COMMIT');self.check()
            self.db.execute('BEGIN');self.begin()
            self.db.execute('DELETE FROM a WHERE id IN(20,21)')
            self.flush();self.db.execute('COMMIT')
        self.db.execute('BEGIN');self.begin()
        for table in self.sources:self.db.execute(f'DELETE FROM {table}')
        self.flush();self.check();self.db.execute('COMMIT');self.check()
    return test
for mode in families.QUERIES:setattr(Batch,'test_'+mode,family(mode))
if __name__=='__main__':unittest.main(verbosity=2)
