"""Declared relational plan used by the public SQLite IVM crossover arm."""


def crossover_plan(name="summary", create_if_missing=False):
    integer = lambda column: {"name": column, "type": "integer", "nullable": False}
    col = lambda name: {"col": name}
    return {
        "version": 1,
        "name": name,
        "schema": [
            {"name": "fact", "columns": list(map(integer, ["id", "group_id", "amount"])),
             "primary_key": ["id"], "create_if_missing": create_if_missing},
            {"name": "dimension", "columns": list(map(integer, ["group_id", "factor"])),
             "primary_key": ["group_id"], "create_if_missing": create_if_missing},
        ],
        "plan": {
            "op": "aggregate",
            "input": {
                "op": "inner_join",
                "left": {"op": "scan", "table": "fact", "as": "f"},
                "right": {"op": "scan", "table": "dimension", "as": "d"},
                "on": {"op": "eq", "left": col("f__group_id"), "right": col("d__group_id")},
            },
            "group_by": [{"name": "group_id", "expr": col("f__group_id")}],
            "aggregates": [
                {"name": "row_count", "fn": "count"},
                {"name": "weighted_sum", "fn": "sum",
                 "expr": {"op": "mul", "left": col("f__amount"), "right": col("d__factor")}},
            ],
        },
    }

