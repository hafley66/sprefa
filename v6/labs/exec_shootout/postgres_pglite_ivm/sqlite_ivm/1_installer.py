"""Atomic installer for persistent pure-SQL SQLite maintenance."""

from __future__ import annotations

import hashlib
import json
import sqlite3
import time
from dataclasses import dataclass
from typing import Any, Callable

from . import plan_module

PlanError = plan_module.PlanError
compile_plan = plan_module.compile_plan
quote = plan_module.quote

ALGORITHM = "full-query-recomputation-per-source-row"
FORMAT_VERSION = 1


@dataclass(frozen=True)
class InstallResult:
    name: str
    query_sql: str
    dependencies: tuple[str, ...]
    output_columns: tuple[str, ...]
    trigger_names: tuple[str, ...]
    algorithm: str = ALGORITHM


def _emit(sink: Callable[[dict[str, Any]], None] | None, phase: str, started: int, **fields: Any) -> None:
    if sink is not None:
        sink({"component": "sqlite_ivm", "phase": phase,
              "duration_ns": time.perf_counter_ns() - started, **fields})


def _object_type(db: sqlite3.Connection, name: str) -> str | None:
    row = db.execute("SELECT type FROM sqlite_schema WHERE name=?", (name,)).fetchone()
    return None if row is None else row[0]


def _source_ddl(table: dict[str, Any]) -> str:
    columns = [f"{quote(column['name'])} INTEGER NOT NULL" for column in table["columns"]]
    if table["primary_key"]:
        columns.append("PRIMARY KEY (" + ", ".join(map(quote, table["primary_key"])) + ")")
    return f"CREATE TABLE {quote(table['name'])} ({', '.join(columns)}) STRICT"


def _validate_source(db: sqlite3.Connection, table: dict[str, Any]) -> None:
    rows = db.execute(f"PRAGMA table_xinfo({quote(table['name'])})").fetchall()
    actual = {row[1]: row for row in rows if row[6] == 0}
    expected = {column["name"] for column in table["columns"]}
    if set(actual) != expected:
        raise PlanError("IVM040_CATALOG_COLUMNS", f"{table['name']} columns are {sorted(actual)}, expected {sorted(expected)}")
    for column in table["columns"]:
        row = actual[column["name"]]
        if "INT" not in row[2].upper() or not (row[3] or row[5]):
            raise PlanError("IVM041_CATALOG_TYPE", f"{table['name']}.{column['name']} must be INTEGER NOT NULL")
        invalid = db.execute(
            f"SELECT 1 FROM {quote(table['name'])} WHERE typeof({quote(column['name'])}) <> 'integer' LIMIT 1"
        ).fetchone()
        if invalid:
            raise PlanError("IVM042_EXISTING_VALUE", f"{table['name']}.{column['name']} contains a non-integer value")


def _metadata_ddl(db: sqlite3.Connection) -> None:
    db.execute("""
      CREATE TABLE IF NOT EXISTS __sqlite_ivm_views(
        name TEXT PRIMARY KEY,
        format_version INTEGER NOT NULL,
        algorithm TEXT NOT NULL,
        plan_json TEXT NOT NULL,
        query_sql TEXT NOT NULL,
        installed_unix_ms INTEGER NOT NULL
      ) STRICT
    """)
    db.execute("""
      CREATE TABLE IF NOT EXISTS __sqlite_ivm_dependencies(
        view_name TEXT NOT NULL,
        source_name TEXT NOT NULL,
        trigger_name TEXT NOT NULL,
        operation TEXT NOT NULL,
        PRIMARY KEY(view_name, trigger_name)
      ) STRICT
    """)
    db.execute("""
      CREATE TABLE IF NOT EXISTS __sqlite_ivm_runtime(
        view_name TEXT PRIMARY KEY,
        refresh_count INTEGER NOT NULL,
        last_source TEXT,
        last_operation TEXT,
        last_changed_unix_s INTEGER,
        last_output_rows INTEGER NOT NULL
      ) STRICT
    """)


def _trigger_name(view: str, source: str, operation: str) -> str:
    digest = hashlib.sha256(f"{view}\0{source}\0{operation}".encode()).hexdigest()[:16]
    return f"__sqlite_ivm_{digest}_{operation.lower()}"


def _literal(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def install(
    db: sqlite3.Connection,
    specification: dict[str, Any],
    *,
    trace: Callable[[dict[str, Any]], None] | None = None,
) -> InstallResult:
    started = time.perf_counter_ns()
    try:
        compiled = compile_plan(specification)
        _emit(trace, "bind-lower", started, status="ok", view=compiled.name,
              dependencies=list(compiled.query.dependencies), algorithm=ALGORITHM)
    except Exception as error:
        _emit(trace, "bind-lower", started, status="error", diagnostic=str(error))
        raise
    canonical = json.dumps(specification, sort_keys=True, separators=(",", ":"))
    savepoint = "sqlite_ivm_install"
    started = time.perf_counter_ns()
    db.execute(f"SAVEPOINT {savepoint}")
    trigger_names = []
    try:
        _metadata_ddl(db)
        if _object_type(db, compiled.name) is not None:
            raise PlanError("IVM043_RESULT_COLLISION", f"result object {compiled.name!r} already exists")
        if db.execute("SELECT 1 FROM __sqlite_ivm_views WHERE name=?", (compiled.name,)).fetchone():
            raise PlanError("IVM044_VIEW_EXISTS", f"view {compiled.name!r} is already installed")
        for table in compiled.schema:
            object_type = _object_type(db, table["name"])
            if object_type is None:
                if not table["create_if_missing"]:
                    raise PlanError("IVM045_SOURCE_MISSING", f"source table {table['name']!r} does not exist")
                db.execute(_source_ddl(table))
            elif object_type != "table":
                raise PlanError("IVM046_SOURCE_KIND", f"source {table['name']!r} is {object_type}, expected table")
            _validate_source(db, table)
        output_ddl = ", ".join(f"{quote(column)} INTEGER" for column in compiled.query.columns)
        db.execute(f"CREATE TABLE {quote(compiled.name)} ({output_ddl}) STRICT")
        db.execute(f"INSERT INTO {quote(compiled.name)} ({', '.join(map(quote, compiled.query.columns))}) {compiled.query.sql}")
        installed_ms = time.time_ns() // 1_000_000
        db.execute(
            "INSERT INTO __sqlite_ivm_views VALUES(?,?,?,?,?,?)",
            (compiled.name, FORMAT_VERSION, ALGORITHM, canonical, compiled.query.sql, installed_ms),
        )
        initial_rows = db.execute(f"SELECT count(*) FROM {quote(compiled.name)}").fetchone()[0]
        db.execute("INSERT INTO __sqlite_ivm_runtime VALUES(?,?,?,?,?,?)",
                   (compiled.name, 0, None, "install", int(time.time()), initial_rows))
        for source in compiled.query.dependencies:
            source_schema = next(table for table in compiled.schema if table["name"] == source)
            type_guard = " OR ".join(
                f"typeof(NEW.{quote(column['name'])}) <> 'integer'"
                for column in source_schema["columns"]
            )
            for operation in ("INSERT", "UPDATE"):
                trigger = _trigger_name(compiled.name, source, "GUARD_" + operation)
                trigger_names.append(trigger)
                db.execute(f"""
                  CREATE TRIGGER {quote(trigger)} BEFORE {operation} ON {quote(source)}
                  WHEN {type_guard}
                  BEGIN
                    SELECT RAISE(ABORT, 'sqlite_ivm requires non-null integer source values');
                  END
                """)
                db.execute("INSERT INTO __sqlite_ivm_dependencies VALUES(?,?,?,?)",
                           (compiled.name, source, trigger, "GUARD_" + operation))
            for operation in ("INSERT", "DELETE", "UPDATE"):
                trigger = _trigger_name(compiled.name, source, operation)
                trigger_names.append(trigger)
                sql = f"""
                  CREATE TRIGGER {quote(trigger)} AFTER {operation} ON {quote(source)} BEGIN
                    DELETE FROM {quote(compiled.name)};
                    INSERT INTO {quote(compiled.name)} ({', '.join(map(quote, compiled.query.columns))}) {compiled.query.sql};
                    UPDATE __sqlite_ivm_runtime
                       SET refresh_count=refresh_count+1,
                           last_source={_literal(source)},
                           last_operation={_literal(operation)},
                           last_changed_unix_s=unixepoch(),
                           last_output_rows=(SELECT count(*) FROM {quote(compiled.name)})
                     WHERE view_name={_literal(compiled.name)};
                  END
                """
                db.execute(sql)
                db.execute("INSERT INTO __sqlite_ivm_dependencies VALUES(?,?,?,?)",
                           (compiled.name, source, trigger, operation))
        db.execute(f"RELEASE {savepoint}")
    except Exception as error:
        db.execute(f"ROLLBACK TO {savepoint}")
        db.execute(f"RELEASE {savepoint}")
        _emit(trace, "install", started, status="error", view=compiled.name,
              diagnostic=str(error), sqlite_errorcode=getattr(error, "sqlite_errorcode", None))
        raise
    _emit(trace, "install", started, status="ok", view=compiled.name,
          triggers=len(trigger_names), output_rows=initial_rows, algorithm=ALGORITHM)
    return InstallResult(compiled.name, compiled.query.sql, compiled.query.dependencies,
                         compiled.query.columns, tuple(trigger_names))


def drop(db: sqlite3.Connection, name: str, *, trace: Callable[[dict[str, Any]], None] | None = None) -> None:
    quote(name)
    started = time.perf_counter_ns()
    db.execute("SAVEPOINT sqlite_ivm_drop")
    try:
        if _object_type(db, "__sqlite_ivm_views") is None:
            raise PlanError("IVM047_NOT_INSTALLED", f"view {name!r} is not installed")
        triggers = [row[0] for row in db.execute(
            "SELECT trigger_name FROM __sqlite_ivm_dependencies WHERE view_name=?", (name,)
        )]
        if not triggers:
            raise PlanError("IVM047_NOT_INSTALLED", f"view {name!r} is not installed")
        for trigger in triggers:
            db.execute(f"DROP TRIGGER {quote(trigger)}")
        db.execute(f"DROP TABLE {quote(name)}")
        db.execute("DELETE FROM __sqlite_ivm_dependencies WHERE view_name=?", (name,))
        db.execute("DELETE FROM __sqlite_ivm_runtime WHERE view_name=?", (name,))
        db.execute("DELETE FROM __sqlite_ivm_views WHERE name=?", (name,))
        db.execute("RELEASE sqlite_ivm_drop")
    except Exception as error:
        db.execute("ROLLBACK TO sqlite_ivm_drop")
        db.execute("RELEASE sqlite_ivm_drop")
        _emit(trace, "drop", started, status="error", view=name, diagnostic=str(error))
        raise
    _emit(trace, "drop", started, status="ok", view=name, triggers=len(triggers))
