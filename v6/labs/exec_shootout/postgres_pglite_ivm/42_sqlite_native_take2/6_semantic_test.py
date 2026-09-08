"""Fresh SQLite SELECT is the oracle after every mutation and rollback."""
import importlib.util
import random
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('boundary', Path(__file__).with_name('3_boundary_test.py'))
boundary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(boundary)

QUERIES = {
    'filter': 'SELECT k,v,count(*) FROM a WHERE v>=0 GROUP BY k,v',
    'bag': 'SELECT k,v,count(*) FROM a GROUP BY k,v',
    'group': 'SELECT k,count(*),sum(v) FROM a GROUP BY k',
    'join': 'SELECT a.k,count(*),sum(a.v*b.v) FROM a JOIN b ON a.k=b.k GROUP BY a.k',
    'self': 'SELECT a.k,count(*),sum(a.v*b.v) FROM a JOIN a b ON a.k=b.k GROUP BY a.k',
    'multi': 'SELECT a.k,count(*),sum(a.v*b.v*c.v) FROM a JOIN b ON a.k=b.k JOIN c ON b.k=c.k GROUP BY a.k',
}

class Semantics(boundary.Boundary):
    # Import only the connection setup helpers, without rerunning boundary cases.
    pass

for name in list(vars(boundary.Boundary)):
    if name.startswith('test_'): setattr(Semantics, name, None)

def family_test(mode):
    def test(self):
        self.db.executescript('CREATE TABLE a(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER); CREATE TABLE b(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER); CREATE TABLE c(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);')
        self.db.execute(f'CREATE VIRTUAL TABLE result USING take2({mode})')
        sources = ['a','b','c'][:{'join':2,'multi':3}.get(mode,1)]
        for side, source in enumerate(sources):
            self.db.execute("SELECT take2_attach('result',?,?, 'id','k','v')", (source,side)).fetchall()

        def check():
            expected = self.db.execute(QUERIES[mode] + ' ORDER BY 1,2,3').fetchall()
            actual = self.db.execute('SELECT * FROM result ORDER BY 1,2,3').fetchall()
            self.assertEqual(actual, expected, mode)
        check()
        for source in sources:
            self.db.execute(f'INSERT INTO {source} VALUES(1,1,2),(2,1,2),(3,2,-3),(4,2,NULL),(5,NULL,4),(6,3,NULL)')
            check()
        self.db.execute('BEGIN')
        for source in sources:
            self.db.execute(f'UPDATE {source} SET k=1,v=3 WHERE id IN (3,4,5)')
            check()
        self.db.execute('SAVEPOINT nested')
        for source in sources:
            self.db.execute(f'DELETE FROM {source} WHERE id IN (1,3)')
            check()
        self.db.execute('ROLLBACK TO nested')
        check()
        self.db.execute('COMMIT')
        check()
        self.db.close()
        self.db = self.connection()
        check()
        before=self.db.execute('SELECT n FROM result_stats').fetchone()
        self.db.execute('BEGIN')
        self.db.execute('UPDATE a SET v=7')
        check()
        self.db.execute("SELECT take2_control('fail_sync')")
        with self.assertRaisesRegex(boundary.sqlite3.DatabaseError,'injected xSync failure'):
            self.db.execute('COMMIT')
        check()
        self.assertEqual(self.db.execute('SELECT n FROM result_stats').fetchone(),before)
        for conflict in ['ABORT','FAIL','IGNORE','REPLACE']:
            statement=f'INSERT OR {conflict} INTO a VALUES(20,1,5),(1,1,5),(21,2,7)'
            if conflict in ['ABORT','FAIL']:
                with self.assertRaises(boundary.sqlite3.IntegrityError): self.db.execute(statement)
            else: self.db.execute(statement)
            check()
            self.db.execute('DELETE FROM a WHERE id IN (20,21)')
            check()
        rng = random.Random(816)
        for step in range(60):
            source = sources[step % len(sources)]
            rowid, key, value = rng.randrange(1,12), rng.choice([None,0,1,2]), rng.choice([None,-2,0,3])
            self.db.execute('SAVEPOINT mutation')
            self.db.execute(f'INSERT INTO {source} VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET k=excluded.k,v=excluded.v', (rowid,key,value))
            check()
            if step % 3 == 0:
                self.db.execute('ROLLBACK TO mutation')
                check()
            self.db.execute('RELEASE mutation')
        for source in sources:
            self.db.execute(f'DELETE FROM {source}')
            check()
        with self.assertRaisesRegex(boundary.sqlite3.DatabaseError,'integer in'):
            self.db.execute('INSERT INTO a VALUES(1,1,1000001)')
        check()
        if mode == 'multi':
            self.db.execute('INSERT INTO a VALUES(1,1,1000000)')
            self.db.execute('INSERT INTO b VALUES(1,1,1000000)')
            with self.assertRaises(boundary.sqlite3.DatabaseError):
                self.db.execute('WITH RECURSIVE x(i) AS(VALUES(1) UNION ALL SELECT i+1 FROM x WHERE i<10) INSERT INTO c SELECT i,1,1000000 FROM x')
            check()
            self.assertEqual(self.db.execute('SELECT count(*) FROM c').fetchone(),(0,))
    return test

for mode in QUERIES:
    setattr(Semantics, 'test_' + mode, family_test(mode))

if __name__ == '__main__': unittest.main(verbosity=2)
