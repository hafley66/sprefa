#!/usr/bin/env python3
"""Universal string relation versus separate path/name dictionaries."""

from __future__ import annotations

import argparse
import json
import sqlite3
import subprocess
import tempfile
import time
from pathlib import Path

from importlib.machinery import SourceFileLoader

_census = SourceFileLoader(
    "file_span_census", str(Path(__file__).with_name("2_census.py"))
).load_module()


def db_bytes(db):
    return db.execute("PRAGMA page_count").fetchone()[0] * db.execute(
        "PRAGMA page_size"
    ).fetchone()[0]


def collect(root, extract, limit):
    paths = _census.tracked_sources(root, limit)
    occurrences = []
    failures = []
    for path_id, relative in enumerate(paths, 1):
        completed = subprocess.run(
            [extract, root / relative], text=True, capture_output=True
        )
        if completed.returncode:
            failures.append((relative, completed.stderr.strip()))
            continue
        for line in completed.stdout.splitlines():
            fact = json.loads(line)
            name = fact.get("name")
            if isinstance(name, str) and name:
                occurrences.append((path_id, name))
    return paths, occurrences, failures


def measure(paths, occurrences, cell):
    with tempfile.TemporaryDirectory(prefix="string-lab-") as temp:
        db = sqlite3.connect(Path(temp) / f"{cell}.sqlite")
        db.execute("PRAGMA journal_mode=OFF")
        db.execute("PRAGMA synchronous=OFF")
        started = time.perf_counter()
        names = sorted({name for _, name in occurrences})
        if cell == "separate":
            name_id = {name: index for index, name in enumerate(names, 1)}
            db.executescript(
                """
                CREATE TABLE path(path_id INTEGER PRIMARY KEY,text TEXT NOT NULL UNIQUE);
                CREATE TABLE name(name_id INTEGER PRIMARY KEY,text TEXT NOT NULL UNIQUE);
                CREATE TABLE occurrence(
                  path_id INTEGER NOT NULL,
                  ordinal INTEGER NOT NULL,
                  name_id INTEGER NOT NULL,
                  PRIMARY KEY(path_id,ordinal)
                ) WITHOUT ROWID;
                CREATE INDEX occurrence_name ON occurrence(name_id,path_id);
                """
            )
            db.executemany("INSERT INTO path VALUES (?,?)", enumerate(paths, 1))
            db.executemany("INSERT INTO name VALUES (?,?)", enumerate(names, 1))
            ordinals = {}
            rows = []
            for path_id, name in occurrences:
                ordinal = ordinals.get(path_id, 0)
                ordinals[path_id] = ordinal + 1
                rows.append((path_id, ordinal, name_id[name]))
            db.executemany("INSERT INTO occurrence VALUES (?,?,?)", rows)
            lookup_sql = "SELECT name_id FROM name WHERE text=?"
        else:
            strings = sorted(set(paths) | set(names))
            string_id = {text: index for index, text in enumerate(strings, 1)}
            db.executescript(
                """
                CREATE TABLE strings(
                  string_id INTEGER PRIMARY KEY,
                  content TEXT NOT NULL UNIQUE
                );
                CREATE TABLE path(
                  path_id INTEGER PRIMARY KEY,
                  string_id INTEGER NOT NULL UNIQUE
                );
                CREATE TABLE occurrence(
                  path_id INTEGER NOT NULL,
                  ordinal INTEGER NOT NULL,
                  name_string_id INTEGER NOT NULL,
                  PRIMARY KEY(path_id,ordinal)
                ) WITHOUT ROWID;
                CREATE INDEX occurrence_name ON occurrence(name_string_id,path_id);
                """
            )
            db.executemany("INSERT INTO strings VALUES (?,?)", enumerate(strings, 1))
            db.executemany(
                "INSERT INTO path VALUES (?,?)",
                ((path_id, string_id[path]) for path_id, path in enumerate(paths, 1)),
            )
            ordinals = {}
            rows = []
            for path_id, name in occurrences:
                ordinal = ordinals.get(path_id, 0)
                ordinals[path_id] = ordinal + 1
                rows.append((path_id, ordinal, string_id[name]))
            db.executemany("INSERT INTO occurrence VALUES (?,?,?)", rows)
            lookup_sql = "SELECT string_id FROM strings WHERE content=?"
        db.commit()
        ingest_ms = (time.perf_counter() - started) * 1000
        needle = names[len(names) // 2] if names else ""
        started = time.perf_counter()
        for _ in range(1000):
            db.execute(lookup_sql, (needle,)).fetchone()
        lookup_us = (time.perf_counter() - started) * 1000
        result = {
            "cell": cell,
            "paths": len(paths),
            "name_occurrences": len(occurrences),
            "distinct_names": len(names),
            "cross_domain_shared_texts": len(set(paths) & set(names)),
            "db_bytes": db_bytes(db),
            "bytes_per_occurrence": db_bytes(db) / max(1, len(occurrences)),
            "ingest_ms": ingest_ms,
            "lookup_us": lookup_us,
            "plan": [
                row[3]
                for row in db.execute(
                    "EXPLAIN QUERY PLAN " + lookup_sql, (needle,)
                ).fetchall()
            ],
        }
        db.close()
        return result


def main(args):
    root = Path(args.root).resolve()
    extract = Path(args.extract).resolve()
    paths, occurrences, failures = collect(root, extract, args.limit)
    result = {
        "failures": failures,
        "cells": [
            measure(paths, occurrences, cell)
            for cell in ("separate", "universal")
        ],
    }
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default="../..")
    parser.add_argument("--extract", default="../sprefa-extract/target/release/extract")
    parser.add_argument("--limit", type=int, default=200)
    parser.add_argument("--output", default="bench/file_span/string-results.json")
    main(parser.parse_args())
