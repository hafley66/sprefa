"""Compile once, then compare extension-owned maintenance with the exact SELECT."""
import collections
import json
import os
from pathlib import Path
import random
import sqlite3
import subprocess
import tempfile
import unittest

COMPILER = os.environ.get("TAKE2_COMPILER", "/tmp/sprefa-sqlite-competitive/compiler-target/debug/take2-sql-compile")
EXTENSION = os.environ["TAKE2_EXTENSION"]
QUERIES = [
    "SELECT a.k+1, CASE WHEN a.v<0 THEN -a.v ELSE a.v END FROM a WHERE a.k%2=0",
    "SELECT x.k-y.k, coalesce(x.v,0)+coalesce(y.v,0) FROM a x JOIN b y ON x.k<y.k WHERE x.v IS NOT NULL",
    "SELECT x.k, y.v FROM a x JOIN a y ON x.v=y.k WHERE x.k<>y.k",
    "SELECT x.k+z.k, y.v-z.v FROM a x JOIN b y ON x.v<y.k JOIN c z ON y.v=z.k WHERE z.v>=0",
    "SELECT x.k, z.v FROM a x JOIN a y ON x.v=y.k JOIN a z ON y.v=z.k",
    'SELECT "left side".k, "right side".v FROM a AS "left side", b AS "right side" WHERE "left side".v>"right side".k',
]


def compile_sql(query):
    process = subprocess.run([COMPILER], input=query, text=True, capture_output=True, check=True)
    return json.loads(process.stdout)


class CompilerOracle(unittest.TestCase):
    def connect(self, path):
        db = sqlite3.connect(path, isolation_level=None)
        db.enable_load_extension(True)
        db.load_extension(EXTENSION)
        db.execute("PRAGMA recursive_triggers=ON")
        db.execute("SELECT take2_control('cache_on')")
        if os.environ.get("TAKE2_SOURCE_VIEWS") == "1":
            db.execute("SELECT take2_control('source_views_on')")
        return db

    def test_shared_select_oracles(self):
        for query in QUERIES:
            with self.subTest(query=query), tempfile.TemporaryDirectory(prefix="compile-", dir="/tmp/sprefa-sqlite-competitive") as root:
                plan = compile_sql(query)
                path = Path(root) / "state.db"
                db = self.connect(path)
                for source in plan["sources"]:
                    db.execute(f'CREATE TABLE "{source}"(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER)')
                db.execute(plan["create_sql"])
                for side, source in enumerate(plan["sources"]):
                    db.execute("SELECT take2_attach('compiled_result',?,?,'id','k','v')", (source, side))
                db.execute("SELECT take2_prepare('compiled_result')")
                def check():
                    columns = 'id,k,v' if plan['plan'].get('aggregate') else 'id,k'
                    self.assertEqual(collections.Counter(db.execute(f"SELECT {columns} FROM compiled_result")), collections.Counter(db.execute(query)))
                rng = random.Random(711)
                db.execute("BEGIN")
                for source in plan["sources"]:
                    db.execute(f'INSERT INTO "{source}" VALUES(10,1,2),(11,2,3),(12,3,1)')
                check()
                for turn in range(36):
                    if turn == 9:
                        db.execute("SAVEPOINT nested")
                    if turn == 18:
                        db.execute("ROLLBACK TO nested")
                        db.execute("RELEASE nested")
                        check()
                    source = plan["sources"][turn % len(plan["sources"]) ]
                    db.execute(f'INSERT INTO "{source}" VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET k=excluded.k,v=excluded.v', (turn % 7, rng.randrange(-2,4), rng.choice([None,-1,0,1,2,3])))
                    check()
                db.execute("COMMIT")
                db.close()
                db = self.connect(path)
                check()
                db.execute("BEGIN")
                for source in plan["sources"]:
                    db.execute(f'DELETE FROM "{source}"')
                check()
                db.execute("ROLLBACK")
                check()
                db.close()

    def test_malformed_plan_rejected(self):
        original = compile_sql(QUERIES[0])["plan"]
        for patch in [{"sides":[4294967296]}, {"version":4294967297}, {"aliases":["a\u0000b"]}, {"predicate":"1; DELETE FROM probe"}]:
            with self.subTest(patch=patch):
                db = self.connect(":memory:")
                encoded = json.dumps(dict(original, **patch)).replace("'", "''")
                with self.assertRaisesRegex(sqlite3.DatabaseError, "plan|binding"):
                    db.execute(f"CREATE VIRTUAL TABLE result USING take2_lazy(plan,'{encoded}')")
                self.assertEqual(db.execute("SELECT name FROM sqlite_schema").fetchall(), [])
                db.close()

    def test_scalar_binding_rejections(self):
        for query in ["SELECT k,random() FROM a", "SELECT id,v FROM a", "SELECT k,(SELECT 1) FROM a", "SELECT k,? FROM a", "SELECT k,sum(v) FROM a"]:
            with self.subTest(query=query):
                plan = compile_sql(query)
                db = self.connect(":memory:")
                with self.assertRaisesRegex(sqlite3.DatabaseError, "plan|binding"):
                    db.execute(plan["create_sql"])
                self.assertEqual(db.execute("SELECT name FROM sqlite_schema").fetchall(), [])
                db.close()

    def test_projection_type_failure_rolls_back(self):
        for value in ["'12'", "1.5"]:
            with self.subTest(value=value):
                db = self.connect(":memory:")
                db.execute("CREATE TABLE a(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER)")
                db.execute(compile_sql(f"SELECT {value},v FROM a")["create_sql"])
                db.execute("SELECT take2_attach('compiled_result','a',0,'id','k','v')")
                db.execute("SELECT take2_prepare('compiled_result')")
                with self.assertRaises(sqlite3.DatabaseError):
                    db.execute("INSERT INTO a VALUES(1,2,3)")
                self.assertEqual(db.execute("SELECT * FROM a").fetchall(), [])
                self.assertEqual(db.execute("SELECT * FROM compiled_result").fetchall(), [])
                db.close()


if __name__ == "__main__":
    unittest.main(verbosity=2)
