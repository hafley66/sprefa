"""Validation and SQL lowering for the declared SQLite IVM relational subset."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


class PlanError(ValueError):
    """A stable admission diagnostic raised before SQLite schema mutation."""

    def __init__(self, code: str, detail: str):
        self.code = code
        self.detail = detail
        super().__init__(f"{code}: {detail}")


def quote(name: str) -> str:
    if not isinstance(name, str) or not name or "\x00" in name:
        raise PlanError("IVM001_IDENTIFIER", f"invalid identifier {name!r}")
    return '"' + name.replace('"', '""') + '"'


@dataclass(frozen=True)
class Lowered:
    sql: str
    columns: tuple[str, ...]
    aggregate: bool
    dependencies: tuple[str, ...]


@dataclass(frozen=True)
class CompiledPlan:
    name: str
    query: Lowered
    schema: tuple[dict[str, Any], ...]


def _need_object(value: Any, where: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise PlanError("IVM002_SHAPE", f"{where} must be an object")
    return value


def _exact_keys(value: dict[str, Any], allowed: set[str], where: str) -> None:
    unknown = sorted(set(value) - allowed)
    if unknown:
        raise PlanError("IVM003_FIELD", f"{where} has unsupported fields: {', '.join(unknown)}")


def _expression(expr: Any, columns: set[str], where: str) -> str:
    expr = _need_object(expr, where)
    if set(expr) == {"col"}:
        name = expr["col"]
        if name not in columns:
            raise PlanError("IVM010_COLUMN", f"{where} references unavailable column {name!r}")
        return quote(name)
    if set(expr) == {"lit"}:
        value = expr["lit"]
        if isinstance(value, bool) or not isinstance(value, int):
            raise PlanError("IVM011_LITERAL", f"{where} only accepts signed integer literals")
        if not -(2**63) <= value < 2**63:
            raise PlanError("IVM012_RANGE", f"{where} literal is outside signed 64-bit range")
        return str(value)
    op = expr.get("op")
    binary = {
        "add": "+", "sub": "-", "mul": "*", "mod": "%",
        "eq": "=", "ne": "<>", "lt": "<", "le": "<=", "gt": ">", "ge": ">=",
        "and": "AND", "or": "OR",
    }
    if op in binary:
        _exact_keys(expr, {"op", "left", "right"}, where)
        left = _expression(expr.get("left"), columns, where + ".left")
        right = _expression(expr.get("right"), columns, where + ".right")
        return f"({left} {binary[op]} {right})"
    if op == "not":
        _exact_keys(expr, {"op", "value"}, where)
        return f"(NOT {_expression(expr.get('value'), columns, where + '.value')})"
    raise PlanError("IVM013_EXPRESSION", f"{where} has unsupported expression operator {op!r}")


def _named_expressions(items: Any, columns: set[str], where: str) -> tuple[list[str], tuple[str, ...]]:
    if not isinstance(items, list) or not items:
        raise PlanError("IVM004_LIST", f"{where} must be a non-empty list")
    sql = []
    names = []
    for index, raw in enumerate(items):
        item = _need_object(raw, f"{where}[{index}]")
        _exact_keys(item, {"name", "expr"}, f"{where}[{index}]")
        name = item.get("name")
        quote(name)
        if name in names:
            raise PlanError("IVM014_DUPLICATE_COLUMN", f"duplicate output column {name!r}")
        names.append(name)
        sql.append(f"{_expression(item.get('expr'), columns, f'{where}[{index}].expr')} AS {quote(name)}")
    return sql, tuple(names)


def _lower(node: Any, tables: dict[str, dict[str, Any]], path: str = "plan") -> Lowered:
    node = _need_object(node, path)
    op = node.get("op")
    if op == "scan":
        _exact_keys(node, {"op", "table", "as"}, path)
        table = node.get("table")
        alias = node.get("as")
        quote(alias)
        if table not in tables:
            raise PlanError("IVM020_TABLE", f"{path} references undeclared table {table!r}")
        columns = tuple(f"{alias}__{column['name']}" for column in tables[table]["columns"])
        select = ", ".join(
            f"{quote(column['name'])} AS {quote(output)}"
            for column, output in zip(tables[table]["columns"], columns)
        )
        return Lowered(f"SELECT {select} FROM {quote(table)}", columns, False, (table,))
    if op == "filter":
        _exact_keys(node, {"op", "input", "predicate"}, path)
        source = _lower(node.get("input"), tables, path + ".input")
        predicate = _expression(node.get("predicate"), set(source.columns), path + ".predicate")
        return Lowered(
            f"SELECT * FROM ({source.sql}) AS {quote('_ivm_filter')} WHERE {predicate}",
            source.columns, source.aggregate, source.dependencies,
        )
    if op == "project":
        _exact_keys(node, {"op", "input", "columns"}, path)
        source = _lower(node.get("input"), tables, path + ".input")
        select, columns = _named_expressions(node.get("columns"), set(source.columns), path + ".columns")
        return Lowered(
            f"SELECT {', '.join(select)} FROM ({source.sql}) AS {quote('_ivm_project')}",
            columns, False, source.dependencies,
        )
    if op == "inner_join":
        _exact_keys(node, {"op", "left", "right", "on"}, path)
        left = _lower(node.get("left"), tables, path + ".left")
        right = _lower(node.get("right"), tables, path + ".right")
        overlap = sorted(set(left.columns) & set(right.columns))
        if overlap:
            raise PlanError("IVM021_JOIN_COLUMN", f"{path} inputs overlap: {', '.join(overlap)}")
        columns = left.columns + right.columns
        predicate = _expression(node.get("on"), set(columns), path + ".on")
        sql = (
            f"SELECT * FROM ({left.sql}) AS {quote('_ivm_left')} "
            f"INNER JOIN ({right.sql}) AS {quote('_ivm_right')} ON {predicate}"
        )
        return Lowered(sql, columns, False, tuple(dict.fromkeys(left.dependencies + right.dependencies)))
    if op == "aggregate":
        _exact_keys(node, {"op", "input", "group_by", "aggregates"}, path)
        source = _lower(node.get("input"), tables, path + ".input")
        group_items = node.get("group_by")
        if not isinstance(group_items, list):
            raise PlanError("IVM004_LIST", f"{path}.group_by must be a list")
        group_sql = []
        names = []
        expressions = []
        for index, raw in enumerate(group_items):
            item = _need_object(raw, f"{path}.group_by[{index}]")
            _exact_keys(item, {"name", "expr"}, f"{path}.group_by[{index}]")
            name = item.get("name")
            quote(name)
            expression = _expression(item.get("expr"), set(source.columns), f"{path}.group_by[{index}].expr")
            if name in names:
                raise PlanError("IVM014_DUPLICATE_COLUMN", f"duplicate output column {name!r}")
            names.append(name)
            expressions.append(expression)
            group_sql.append(f"{expression} AS {quote(name)}")
        aggregates = node.get("aggregates")
        if not isinstance(aggregates, list) or not aggregates:
            raise PlanError("IVM004_LIST", f"{path}.aggregates must be a non-empty list")
        aggregate_sql = []
        for index, raw in enumerate(aggregates):
            item = _need_object(raw, f"{path}.aggregates[{index}]")
            function = item.get("fn")
            allowed = {"name", "fn"} if function == "count" else {"name", "fn", "expr"}
            _exact_keys(item, allowed, f"{path}.aggregates[{index}]")
            name = item.get("name")
            quote(name)
            if name in names:
                raise PlanError("IVM014_DUPLICATE_COLUMN", f"duplicate output column {name!r}")
            names.append(name)
            if function == "count":
                aggregate_sql.append(f"count(*) AS {quote(name)}")
            elif function == "sum":
                expression = _expression(item.get("expr"), set(source.columns), f"{path}.aggregates[{index}].expr")
                aggregate_sql.append(f"sum({expression}) AS {quote(name)}")
            else:
                raise PlanError("IVM030_AGGREGATE", f"unsupported aggregate {function!r}")
        select = ", ".join(group_sql + aggregate_sql)
        group = " GROUP BY " + ", ".join(expressions) if expressions else ""
        return Lowered(
            f"SELECT {select} FROM ({source.sql}) AS {quote('_ivm_aggregate')}{group}",
            tuple(names), True, source.dependencies,
        )
    raise PlanError("IVM031_OPERATOR", f"{path} has unsupported relational operator {op!r}")


def compile_plan(specification: Any) -> CompiledPlan:
    specification = _need_object(specification, "specification")
    _exact_keys(specification, {"version", "name", "schema", "plan"}, "specification")
    if specification.get("version") != 1:
        raise PlanError("IVM000_VERSION", "only relational plan version 1 is supported")
    name = specification.get("name")
    quote(name)
    if name.startswith("__sqlite_ivm_"):
        raise PlanError("IVM001_IDENTIFIER", "view names using the __sqlite_ivm_ prefix are reserved")
    raw_schema = specification.get("schema")
    if not isinstance(raw_schema, list) or not raw_schema:
        raise PlanError("IVM004_LIST", "schema must be a non-empty list")
    tables = {}
    normalized = []
    for index, raw in enumerate(raw_schema):
        table = _need_object(raw, f"schema[{index}]")
        _exact_keys(table, {"name", "columns", "primary_key", "create_if_missing"}, f"schema[{index}]")
        table_name = table.get("name")
        quote(table_name)
        if table_name in tables or table_name == name:
            raise PlanError("IVM022_SCHEMA_COLLISION", f"duplicate or result-colliding table {table_name!r}")
        columns = table.get("columns")
        if not isinstance(columns, list) or not columns:
            raise PlanError("IVM004_LIST", f"schema[{index}].columns must be a non-empty list")
        seen = set()
        normalized_columns = []
        for column_index, raw_column in enumerate(columns):
            column = _need_object(raw_column, f"schema[{index}].columns[{column_index}]")
            _exact_keys(column, {"name", "type", "nullable"}, f"schema[{index}].columns[{column_index}]")
            column_name = column.get("name")
            quote(column_name)
            if column_name in seen:
                raise PlanError("IVM014_DUPLICATE_COLUMN", f"duplicate source column {column_name!r}")
            seen.add(column_name)
            if column.get("type") != "integer":
                raise PlanError("IVM023_TYPE", f"{table_name}.{column_name} must declare integer")
            if column.get("nullable") is not False:
                raise PlanError("IVM024_NULL", f"{table_name}.{column_name} must be non-null")
            normalized_columns.append({"name": column_name, "type": "integer", "nullable": False})
        primary_key = table.get("primary_key", [])
        if not isinstance(primary_key, list) or any(column not in seen for column in primary_key):
            raise PlanError("IVM025_KEY", f"{table_name} primary_key must name declared columns")
        normalized_table = {
            "name": table_name,
            "columns": normalized_columns,
            "primary_key": primary_key,
            "create_if_missing": table.get("create_if_missing", False) is True,
        }
        tables[table_name] = normalized_table
        normalized.append(normalized_table)
    query = _lower(specification.get("plan"), tables)
    return CompiledPlan(name, query, tuple(normalized))

