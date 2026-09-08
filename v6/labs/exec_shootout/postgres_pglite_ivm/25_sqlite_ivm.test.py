"""Public declared-plan SQLite IVM API and admitted semantic transitions."""

import copy
import sqlite3
import tempfile
import unittest
from pathlib import Path

from sqlite_ivm import PlanError, compile_plan, drop, install


def column(name):
    return {"col": name}


def binary(op, left, right):
    return {"op": op, "left": left, "right": right}


def schema(*tables):
    return [
        {
            "name": name,
            "columns": [{"name": column_name, "type": "integer", "nullable": False} for column_name in columns],
            "primary_key": key,
            "create_if_missing": True,
        }
        for name, columns, key in tables
    ]


def crossover_spec(name="summary"):
    joined = {
        "op": "inner_join",
        "left": {"op": "scan", "table": "fact", "as": "f"},
        "right": {"op": "scan", "table": "dimension", "as": "d"},
        "on": binary("eq", column("f__group_id"), column("d__group_id")),
    }
    return {
        "version": 1,
        "name": name,
        "schema": schema(
            ("fact", ["id", "group_id", "amount"], ["id"]),
            ("dimension", ["group_id", "factor"], ["group_id"]),
        ),
        "plan": {
            "op": "aggregate",
            "input": {"op": "filter", "input": joined,
                      "predicate": binary("ge", column("f__amount"), {"lit": -1000})},
            "group_by": [{"name": "group_id", "expr": column("f__group_id")}],
            "aggregates": [
                {"name": "row_count", "fn": "count"},
                {"name": "weighted_sum", "fn": "sum",
                 "expr": binary("mul", column("f__amount"), column("d__factor"))},
            ],
        },
    }


class SqliteIvm(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="sqlite-ivm-public-")
        self.path = Path(self.temp.name) / "state.sqlite"
        self.db = sqlite3.connect(self.path, isolation_level=None)

    def tearDown(self):
        self.db.close()
        self.temp.cleanup()

    def test_grouped_join_all_transitions_duplicates_empty_group_and_rollback(self):
        events = []
        result = install(self.db, crossover_spec(), trace=events.append)
        self.assertEqual(result.algorithm, "full-query-recomputation-per-source-row")
        self.db.executemany("INSERT INTO dimension VALUES(?,?)", [(1, 2), (2, 0), (3, -3)])
        self.db.executemany("INSERT INTO fact VALUES(?,?,?)", [(1, 1, 5), (2, 1, 5), (3, 2, 9), (4, 3, -2)])
        self.assertEqual(self.db.execute("SELECT * FROM summary ORDER BY 1").fetchall(),
                         [(1, 2, 20), (2, 1, 0), (3, 1, 6)])
        self.db.execute("UPDATE fact SET id=20,group_id=3,amount=4 WHERE id=1")
        self.db.execute("UPDATE dimension SET factor=4 WHERE group_id=1")
        self.assertEqual(self.db.execute("SELECT * FROM summary ORDER BY 1").fetchall(),
                         [(1, 1, 20), (2, 1, 0), (3, 2, -6)])
        self.db.execute("DELETE FROM fact WHERE group_id=2")
        self.assertEqual(self.db.execute("SELECT * FROM summary ORDER BY 1").fetchall(),
                         [(1, 1, 20), (3, 2, -6)])
        before = self.db.execute("SELECT * FROM summary ORDER BY 1").fetchall()
        self.db.execute("BEGIN")
        self.db.execute("DELETE FROM fact")
        self.assertEqual(self.db.execute("SELECT * FROM summary").fetchall(), [])
        self.db.execute("ROLLBACK")
        self.assertEqual(self.db.execute("SELECT * FROM summary ORDER BY 1").fetchall(), before)
        telemetry = self.db.execute(
            "SELECT refresh_count,last_source,last_operation,last_output_rows FROM __sqlite_ivm_runtime WHERE view_name='summary'"
        ).fetchone()
        self.assertEqual(telemetry[1:], ("fact", "DELETE", 2))
        self.assertGreater(telemetry[0], 0)
        self.assertEqual([event["phase"] for event in events], ["bind-lower", "install"])

    def test_projection_bag_self_join_and_multiway_join(self):
        spec = {
            "version": 1,
            "name": "pairs",
            "schema": schema(
                ("edge", ["id", "src", "dst"], ["id"]),
                ("label", ["node", "kind"], ["node"]),
            ),
            "plan": {
                "op": "project",
                "input": {
                    "op": "inner_join",
                    "left": {
                        "op": "inner_join",
                        "left": {"op": "scan", "table": "edge", "as": "a"},
                        "right": {"op": "scan", "table": "edge", "as": "b"},
                        "on": binary("eq", column("a__dst"), column("b__src")),
                    },
                    "right": {"op": "scan", "table": "label", "as": "l"},
                    "on": binary("eq", column("b__dst"), column("l__node")),
                },
                "columns": [
                    {"name": "src", "expr": column("a__src")},
                    {"name": "dst", "expr": column("b__dst")},
                    {"name": "kind", "expr": column("l__kind")},
                ],
            },
        }
        install(self.db, spec)
        self.db.executemany("INSERT INTO edge VALUES(?,?,?)", [(1, 1, 2), (2, 1, 2), (3, 2, 3), (4, 2, 3)])
        self.db.execute("INSERT INTO label VALUES(3,7)")
        self.assertEqual(self.db.execute("SELECT * FROM pairs ORDER BY rowid").fetchall(), [(1, 3, 7)] * 4)
        self.db.execute("DELETE FROM edge WHERE id=4")
        self.assertEqual(self.db.execute("SELECT * FROM pairs ORDER BY rowid").fetchall(), [(1, 3, 7)] * 2)

    def test_global_aggregate_empty_boundary_multiple_views_reopen_and_drop(self):
        spec = crossover_spec("global_total")
        spec["plan"]["group_by"] = []
        install(self.db, spec)
        install(self.db, crossover_spec("group_total"))
        self.assertEqual(self.db.execute("SELECT * FROM global_total").fetchall(), [(0, None)])
        self.db.execute("INSERT INTO dimension VALUES(1,2)")
        self.db.execute("INSERT INTO fact VALUES(1,1,8)")
        self.assertEqual(self.db.execute("SELECT * FROM global_total").fetchall(), [(1, 16)])
        self.assertEqual(self.db.execute("SELECT * FROM group_total").fetchall(), [(1, 1, 16)])
        self.db.close()
        self.db = sqlite3.connect(self.path, isolation_level=None)
        self.db.execute("UPDATE fact SET amount=-3 WHERE id=1")
        self.assertEqual(self.db.execute("SELECT * FROM global_total").fetchall(), [(1, -6)])
        drop(self.db, "group_total")
        self.assertIsNone(self.db.execute("SELECT type FROM sqlite_schema WHERE name='group_total'").fetchone())
        self.db.execute("UPDATE fact SET amount=4 WHERE id=1")
        self.assertEqual(self.db.execute("SELECT * FROM global_total").fetchall(), [(1, 8)])

    def test_storage_contract_overflow_and_atomic_install_failure(self):
        install(self.db, crossover_spec())
        self.db.execute("INSERT INTO dimension VALUES(1,2)")
        before = self.db.execute("SELECT * FROM summary").fetchall()
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("INSERT INTO fact VALUES(1,1,'bad')")
        self.assertEqual(self.db.execute("SELECT * FROM summary").fetchall(), before)
        with self.assertRaises(sqlite3.Error):
            self.db.execute("INSERT INTO fact VALUES(1,1,9223372036854775807)")
        self.assertEqual(self.db.execute("SELECT * FROM fact").fetchall(), [])
        collision = crossover_spec("fact")
        with self.assertRaisesRegex(PlanError, "IVM022_SCHEMA_COLLISION"):
            compile_plan(collision)
        broken = crossover_spec("broken")
        broken["schema"][0]["create_if_missing"] = False
        other = sqlite3.connect(":memory:", isolation_level=None)
        try:
            with self.assertRaisesRegex(PlanError, "IVM045_SOURCE_MISSING"):
                install(other, broken)
            self.assertEqual(other.execute("SELECT name FROM sqlite_schema").fetchall(), [])
        finally:
            other.close()

    def test_rejected_relational_and_expression_classes_have_codes(self):
        cases = []
        for operator in ["left_join", "semi_join", "anti_join", "union", "distinct", "order", "limit",
                         "window", "recursive", "cte", "scalar_subquery", "exists"]:
            spec = crossover_spec()
            spec["plan"] = {"op": operator}
            cases.append((spec, "IVM031_OPERATOR"))
        aggregate = crossover_spec()
        aggregate["plan"]["aggregates"][0] = {"name": "row_count", "fn": "avg", "expr": column("f__amount")}
        cases.append((aggregate, "IVM030_AGGREGATE"))
        expression = crossover_spec()
        expression["plan"]["input"]["predicate"] = {"op": "function", "name": "random"}
        cases.append((expression, "IVM013_EXPRESSION"))
        nullable = crossover_spec()
        nullable["schema"][0]["columns"][2]["nullable"] = True
        cases.append((nullable, "IVM024_NULL"))
        text = crossover_spec()
        text["schema"][0]["columns"][2]["type"] = "text"
        cases.append((text, "IVM023_TYPE"))
        for spec, code in cases:
            with self.subTest(code=code, op=spec["plan"].get("op")):
                with self.assertRaisesRegex(PlanError, code):
                    compile_plan(spec)


if __name__ == "__main__":
    unittest.main(verbosity=2)
