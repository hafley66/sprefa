"""Shared crossover transport through the public declared-plan SQLite IVM API."""

import argparse
import hashlib
import importlib.util
import json
import os
import resource
import sqlite3
import sys
import time
from pathlib import Path

from sqlite_ivm import install

LAB = Path(__file__).resolve().parent
plan_spec = importlib.util.spec_from_file_location("crossover_plan", LAB / "26_crossover_plan.py")
plan_module = importlib.util.module_from_spec(plan_spec)
plan_spec.loader.exec_module(plan_module)


def log(event, **fields):
    print(json.dumps({"event": event, "status": "ok", **fields}), flush=True)


def trace_sink(event):
    if os.environ.get("SQLITE_IVM_TRACE") == "1":
        print(json.dumps(event, sort_keys=True), file=sys.stderr, flush=True)


def configure(db):
    values = {}
    for pragma, value in [("journal_mode", "WAL"), ("synchronous", "FULL"),
                          ("foreign_keys", "ON"), ("temp_store", "MEMORY")]:
        row = db.execute(f"PRAGMA {pragma}={value}").fetchone()
        values[pragma] = value if row is None else row[0]
    values["cache_size"] = db.execute("PRAGMA cache_size").fetchone()[0]
    values["mmap_size"] = db.execute("PRAGMA mmap_size").fetchone()[0]
    return values


def create_sources(db, initial):
    db.execute("CREATE TABLE fact(id INTEGER NOT NULL PRIMARY KEY, group_id INTEGER NOT NULL, amount INTEGER NOT NULL) STRICT")
    db.execute("CREATE TABLE dimension(group_id INTEGER NOT NULL PRIMARY KEY, factor INTEGER NOT NULL) STRICT")
    db.executemany("INSERT INTO dimension VALUES(?,?)", initial["dimension"])
    db.executemany("INSERT INTO fact VALUES(?,?,?)", initial["fact"])
    db.execute("CREATE INDEX fact_group_idx ON fact(group_id)")


def read_state(db):
    return {
        "dimension": db.execute("SELECT group_id,factor FROM dimension ORDER BY group_id").fetchall(),
        "fact": db.execute("SELECT id,group_id,amount FROM fact ORDER BY id").fetchall(),
        "summary": db.execute("SELECT group_id,row_count,weighted_sum FROM summary ORDER BY group_id").fetchall(),
    }


def verify(db, fixture_state):
    actual = read_state(db)
    for relation in ("dimension", "fact"):
        expected = list(map(tuple, fixture_state["inputs"][relation]))
        if actual[relation] != expected:
            raise AssertionError((relation, actual[relation], expected))
    dimensions = dict(actual["dimension"])
    expected_summary = {}
    for _, group, amount in actual["fact"]:
        if group in dimensions:
            count, total = expected_summary.get(group, (0, 0))
            expected_summary[group] = count + 1, total + amount * dimensions[group]
    expected = sorted((group, *value) for group, value in expected_summary.items())
    if actual["summary"] != expected:
        raise AssertionError((actual["summary"], expected))
    canonical = "\n".join("S\t" + "\t".join(map(str, row)) for row in expected)
    checksum = hashlib.sha256(canonical.encode()).hexdigest()
    if checksum != fixture_state["expected"]["checksum"]:
        raise AssertionError((checksum, fixture_state["expected"]["checksum"]))
    input_text = "\n".join(
        ["D\t" + "\t".join(map(str, row)) for row in actual["dimension"]]
        + ["F\t" + "\t".join(map(str, row)) for row in actual["fact"]]
    )
    input_hash = hashlib.sha256(input_text.encode()).hexdigest()
    if input_hash != fixture_state["input_hash"]:
        raise AssertionError((input_hash, fixture_state["input_hash"]))
    return actual, checksum, input_hash, len(canonical.encode())


def file_size(path):
    return path.stat().st_size if path.exists() else 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", required=True)
    parser.add_argument("--db", required=True)
    parser.add_argument("--sql-output")
    args = parser.parse_args()
    db_path = Path(args.db)
    if db_path.exists():
        raise ValueError("refusing to overwrite prior SQLite receipt")
    fixture_path = Path(args.fixture)
    fixture = json.loads(fixture_path.read_text())
    db = sqlite3.connect(db_path, isolation_level=None)
    setup_started = time.perf_counter_ns()
    configuration = configure(db)
    create_sources(db, fixture["states"][0]["inputs"])
    result = install(db, plan_module.crossover_plan(), trace=trace_sink)
    setup_ms = (time.perf_counter_ns() - setup_started) / 1_000_000
    if args.sql_output:
        rows = db.execute(
            "SELECT type,name,sql FROM sqlite_schema WHERE name='summary' OR name LIKE '__sqlite_ivm_%' ORDER BY type,name"
        ).fetchall()
        Path(args.sql_output).write_text("\n\n".join(row[2] + ";" for row in rows if row[2]) + "\n")
    log("case-setup", setup_ms=setup_ms, runtime="stock SQLite " + sqlite3.sqlite_version,
        algorithm=result.algorithm, sql_consumer_install_api=True,
        durability="on-disk WAL/synchronous=FULL", sqlite_configuration=configuration,
        plan_sha256=hashlib.sha256(json.dumps(plan_module.crossover_plan(), sort_keys=True).encode()).hexdigest(),
        fixture_sha256=hashlib.sha256(fixture_path.read_bytes()).hexdigest(),
        memory_scope="Python process including SQLite, fixture, oracle and public installer",
        total_memory_enforcement="UNENFORCED")
    total = 0.0
    durable_commit_total = 0.0
    for state in fixture["states"]:
        affected = 0
        apply_ms = 0.0
        commit_ms = 0.0
        if state["name"] != "initial":
            db.execute("BEGIN IMMEDIATE")
            apply_started = time.perf_counter_ns()
            try:
                db.execute(state["mutation_sql"])
                affected = db.execute("SELECT changes()").fetchone()[0]
                apply_ms = (time.perf_counter_ns() - apply_started) / 1_000_000
                commit_started = time.perf_counter_ns()
                db.execute("COMMIT")
                commit_ms = (time.perf_counter_ns() - commit_started) / 1_000_000
            except Exception:
                db.execute("ROLLBACK")
                raise
        query_started = time.perf_counter_ns()
        db.execute("CREATE TEMP TABLE crossover_snapshot AS SELECT * FROM summary")
        materialized_count = db.execute("SELECT count(*) FROM crossover_snapshot").fetchone()[0]
        query_ms = (time.perf_counter_ns() - query_started) / 1_000_000
        check_started = time.perf_counter_ns()
        actual, checksum, input_hash, output_bytes = verify(db, state)
        check_ms = (time.perf_counter_ns() - check_started) / 1_000_000
        db.execute("DROP TABLE crossover_snapshot")
        if affected != state["expected_affected_rows"]:
            raise AssertionError((affected, state["expected_affected_rows"]))
        update_ms = apply_ms + commit_ms
        if state["name"] != "initial":
            total += update_ms + query_ms
            durable_commit_total += commit_ms
        runtime = db.execute(
            "SELECT refresh_count,last_source,last_operation,last_output_rows FROM __sqlite_ivm_runtime WHERE view_name='summary'"
        ).fetchone()
        log("mutation", state=state["name"], affected_rows=affected,
            join_affected_rows=state["join_affected_rows"], maintenance_apply_ms=apply_ms,
            durable_commit_ms=commit_ms, update_transaction_ms=update_ms,
            query_compute_ms=query_ms, check_ms=check_ms, update_plus_query_ms=update_ms + query_ms,
            checksum=checksum, input_hash=input_hash, output_rows=len(actual["summary"]),
            materialized_count=materialized_count, output_bytes=output_bytes,
            exact_input_output_validated=True, summary=actual["summary"],
            maintenance_refresh_count=runtime[0], maintenance_last_source=runtime[1],
            maintenance_last_operation=runtime[2], maintenance_last_output_rows=runtime[3])
    db.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    db.close()
    db = sqlite3.connect(db_path, isolation_level=None)
    verify(db, fixture["states"][-1])
    db.close()
    log("case-total", update_plus_query_ms=total, durable_commit_ms=durable_commit_total,
        final_checksum=checksum, final_input_hash=input_hash, fresh_reopen_validated=True,
        disk={"database_bytes": file_size(db_path), "wal_bytes": file_size(Path(str(db_path) + "-wal")),
              "shm_bytes": file_size(Path(str(db_path) + "-shm"))},
        process_peak_rss_platform_units=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
        rss_units="bytes" if sys.platform == "darwin" else "KiB")


if __name__ == "__main__":
    main()

