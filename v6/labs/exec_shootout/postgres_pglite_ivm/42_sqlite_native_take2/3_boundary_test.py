"""SQL-only transport and exact lifecycle assertions; maintenance lives in C."""
import json
import os
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
EXT = os.environ['TAKE2_EXTENSION']

class Boundary(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='boundary-', dir=os.environ['TAKE2_SCRATCH'])
        self.path = str(Path(self.tmp.name) / 'test.db')
        self.db = self.connection()
        self.db.executescript((HERE / '2_boundary.sql').read_text())
        self.trace()

    def tearDown(self):
        self.db.close()
        self.tmp.cleanup()

    def connection(self, load=True):
        db = sqlite3.connect(self.path, isolation_level=None, timeout=0.1)
        if load:
            db.enable_load_extension(True)
            db.load_extension(EXT)
        db.execute('PRAGMA recursive_triggers=ON')
        return db

    def trace(self):
        trace = json.loads(self.db.execute("SELECT take2_control('trace')").fetchone()[0])
        print(json.dumps({'test': self.id(), 'trace': trace}), flush=True)
        return trace

    def equal(self, expected):
        for relation in ['source', 'maintained', 'maintained_state']:
            self.assertEqual(self.db.execute(f'SELECT id,k,v FROM {relation} ORDER BY id').fetchall(), expected, relation)

    def test_autocommit_trace(self):
        self.db.execute('INSERT INTO source VALUES(1,10,100),(2,20,200)')
        self.assertEqual(self.trace(), [['begin',-1],['update',4],['savepoint',0],['release',0],['update',4],['savepoint',0],['release',0],['sync',-1],['commit',-1]])
        self.equal([(1,10,100),(2,20,200)])

    def test_explicit_visibility(self):
        self.db.execute('BEGIN')
        self.db.execute('INSERT INTO source VALUES(1,10,100),(2,20,200)')
        self.equal([(1,10,100),(2,20,200)])
        self.db.execute('UPDATE source SET v=v+1')
        self.equal([(1,10,101),(2,20,201)])
        self.db.execute('DELETE FROM source WHERE id=1')
        self.equal([(2,20,201)])
        self.db.execute('COMMIT')
        self.equal([(2,20,201)])

    def test_savepoints(self):
        self.db.execute('BEGIN')
        self.db.execute('SAVEPOINT a')
        self.db.execute('INSERT INTO source VALUES(1,10,100)')
        self.db.execute('SAVEPOINT b')
        self.db.execute('INSERT INTO source VALUES(2,20,200)')
        self.db.execute('RELEASE b')
        self.db.execute('SAVEPOINT c')
        self.db.execute('INSERT INTO source VALUES(3,30,300)')
        self.db.execute('ROLLBACK TO c')
        self.equal([(1,10,100),(2,20,200)])
        self.db.execute('ROLLBACK TO a')
        self.equal([])
        self.assertEqual(self.db.execute('SELECT n FROM maintained_stats').fetchone(),(0,))
        self.db.execute('RELEASE a')
        self.db.execute('ROLLBACK')
        self.equal([])
        self.assertEqual(self.trace(), [
            ['begin',-1],['savepoint',1],['update',4],['savepoint',2],['release',2],['release',1],
            ['savepoint',1],['savepoint',2],['update',4],['savepoint',3],['release',3],['release',2],['release',1],
            ['savepoint',1],['savepoint',2],['update',4],['savepoint',3],['release',3],['release',2],
            ['rollbackto',1],['filter',-1],['rollbackto',0],['filter',-1],['release',0],['rollback',-1],['filter',-1]])

    def test_conflicts(self):
        for mode, expected in [('ABORT', []), ('FAIL', [(1,10,100)]), ('IGNORE', [(1,10,100),(3,30,300)]), ('REPLACE', [(2,10,200),(3,30,300)])]:
            with self.subTest(mode=mode):
                self.db.execute('DELETE FROM source')
                self.trace()
                self.db.execute('BEGIN')
                statement = f'INSERT OR {mode} INTO source VALUES(1,10,100),(2,10,200),(3,30,300)'
                if mode in ('ABORT','FAIL'):
                    with self.assertRaises(sqlite3.IntegrityError): self.db.execute(statement)
                else: self.db.execute(statement)
                self.equal(expected)
                self.db.execute('COMMIT')
                self.equal(expected)
                policy={'ABORT':4,'FAIL':3,'IGNORE':2,'REPLACE':5}[mode]
                count={'ABORT':1,'FAIL':1,'IGNORE':2,'REPLACE':4}[mode]
                events=[['begin',-1],['savepoint',0]]
                events += [['update',policy],['savepoint',1],['release',1]] * count
                if mode=='ABORT': events += [['rollbackto',0]]
                events += [['release',0],['filter',-1],['sync',-1],['commit',-1],['filter',-1]]
                self.assertEqual(self.trace(),events)

    def test_upsert_check_rollback(self):
        self.db.execute('INSERT INTO source VALUES(1,10,100)')
        self.db.execute('INSERT INTO source VALUES(2,10,200) ON CONFLICT(k) DO UPDATE SET v=excluded.v')
        self.equal([(1,10,200)])
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('INSERT INTO source VALUES(3,30,300),(4,40,-1)')
        self.equal([(1,10,200)])
        self.db.execute('BEGIN')
        self.db.execute('UPDATE source SET v=201')
        self.db.execute('ROLLBACK')
        self.equal([(1,10,200)])

    def test_sync_failure(self):
        for explicit in (False, True):
            with self.subTest(explicit=explicit):
                self.trace()
                self.db.execute("SELECT take2_control('fail_sync')")
                if explicit:
                    self.db.execute('BEGIN')
                    self.db.execute('INSERT INTO source VALUES(1,10,100)')
                    self.equal([(1,10,100)])
                    statement = 'COMMIT'
                else: statement = 'INSERT INTO source VALUES(1,10,100)'
                with self.assertRaisesRegex(sqlite3.DatabaseError,'injected xSync failure'):
                    self.db.execute(statement)
                self.assertFalse(self.db.in_transaction)
                self.equal([])
                self.assertEqual(self.db.execute('SELECT n FROM maintained_stats').fetchone(), (0,))
                expected=[['begin',-1]]
                if explicit: expected += [['savepoint',0]]
                depth=1 if explicit else 0
                expected += [['update',4],['savepoint',depth],['release',depth]]
                if explicit: expected += [['release',0],['filter',-1]]
                expected += [['sync',-1],['rollback',-1],['filter',-1]]
                self.assertEqual(self.trace(),expected)

    def test_connections(self):
        self.db.execute('PRAGMA journal_mode=WAL')
        self.db.execute('INSERT INTO source VALUES(1,10,100)')
        self.db.close()
        self.db = self.connection()
        self.equal([(1,10,100)])
        absent = self.connection(False)
        with self.assertRaisesRegex(sqlite3.DatabaseError,'no such module'):
            absent.execute('INSERT INTO source VALUES(2,20,200)')
        self.assertEqual(absent.execute('SELECT * FROM source').fetchall(),[(1,10,100)])
        absent.close()
        other = self.connection()
        self.db.execute('BEGIN IMMEDIATE')
        self.db.execute('UPDATE source SET v=101')
        self.assertEqual(other.execute('SELECT * FROM maintained').fetchall(),[(1,10,100)])
        with self.assertRaisesRegex(sqlite3.OperationalError,'locked'):
            other.execute('UPDATE source SET v=102')
        self.db.execute('COMMIT')
        other.execute('UPDATE source SET v=102')
        other.close()
        self.equal([(1,10,102)])

    def test_guard_and_shadow_failure(self):
        self.db.execute('PRAGMA recursive_triggers=OFF')
        with self.assertRaisesRegex(sqlite3.DatabaseError,'recursive_triggers=ON'):
            self.db.execute('INSERT INTO source VALUES(1,10,100)')
        self.equal([])
        self.db.execute('PRAGMA recursive_triggers=ON')
        self.db.execute('INSERT INTO source VALUES(1,10,100)')
        self.db.execute('UPDATE maintained_state SET v=99')
        with self.assertRaisesRegex(sqlite3.DatabaseError,'OLD shadow mismatch'):
            self.db.execute('UPDATE source SET v=101')
        self.assertEqual(self.db.execute('SELECT v FROM source').fetchall(),[(100,)])

    def test_direct_event_probe(self):
        self.db.execute('INSERT INTO maintained(id,k,v,op) VALUES(1,10,100,1)')
        self.assertEqual(self.db.execute('SELECT * FROM maintained').fetchall(),[(1,10,100)])
        self.assertEqual(self.db.execute('SELECT * FROM source').fetchall(),[])
        with self.assertRaisesRegex(sqlite3.DatabaseError,'only forwarded'):
            self.db.execute('DELETE FROM maintained')

    def test_shadow_callback_reentry_rejected(self):
        self.db.execute('CREATE TRIGGER bad_shadow AFTER INSERT ON maintained_state BEGIN INSERT INTO maintained(id,k,v,op) VALUES(99,99,99,1); END')
        with self.assertRaises(sqlite3.DatabaseError):
            self.db.execute('INSERT INTO source VALUES(1,10,100)')
        self.equal([])
        self.assertEqual(self.db.execute('SELECT n FROM maintained_stats').fetchone(),(0,))

    def test_deferred_foreign_key(self):
        self.db.execute('PRAGMA foreign_keys=ON')
        self.db.executescript('CREATE TABLE parent(k INTEGER PRIMARY KEY); CREATE TABLE child(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER,FOREIGN KEY(k) REFERENCES parent(k) DEFERRABLE INITIALLY DEFERRED); CREATE VIRTUAL TABLE fk_result USING take2;')
        self.db.execute("SELECT take2_attach('fk_result','child',0,'id','k','v')").fetchall()
        self.db.execute('BEGIN')
        self.db.execute('INSERT INTO child VALUES(1,99,3)')
        self.assertEqual(self.db.execute('SELECT * FROM fk_result').fetchall(),[(1,99,3)])
        with self.assertRaisesRegex(sqlite3.IntegrityError,'FOREIGN KEY'):
            self.db.execute('COMMIT')
        self.assertTrue(self.db.in_transaction)
        self.db.execute('ROLLBACK')
        self.assertEqual(self.db.execute('SELECT * FROM fk_result').fetchall(),[])
        self.assertEqual(self.db.execute('SELECT * FROM child').fetchall(),[])

    def test_outer_savepoint_release(self):
        self.db.execute('SAVEPOINT outermost')
        self.db.execute('INSERT INTO source VALUES(1,10,100)')
        self.equal([(1,10,100)])
        self.db.execute('RELEASE outermost')
        self.assertFalse(self.db.in_transaction)
        self.db.close()
        self.db=self.connection()
        self.equal([(1,10,100)])

    def test_trace_bound(self):
        self.db.execute('WITH RECURSIVE x(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM x WHERE i<300) INSERT INTO source SELECT i,i,i FROM x')
        self.assertEqual(len(self.trace()),256)
        self.assertEqual(self.db.execute('SELECT n FROM maintained_stats').fetchone(),(300,))

if __name__ == '__main__':
    print(json.dumps({'sqlite': sqlite3.sqlite_version, 'extension': EXT}), flush=True)
    unittest.main(verbosity=2)
