#!/usr/bin/env python3
"""Whole-path interning versus segment normalization and repeated path text."""

from __future__ import annotations

import argparse
import json
import sqlite3
import subprocess
import tempfile
import time
from pathlib import Path


def db_bytes(db):
    return db.execute("PRAGMA page_count").fetchone()[0] * db.execute(
        "PRAGMA page_size"
    ).fetchone()[0]


def measure(paths, references, cell):
    with tempfile.TemporaryDirectory(prefix="path-lab-") as temp:
        db = sqlite3.connect(Path(temp) / f"{cell}.sqlite")
        db.execute("PRAGMA journal_mode=OFF")
        db.execute("PRAGMA synchronous=OFF")
        started = time.perf_counter()
        if cell == "whole":
            db.executescript(
                """
                CREATE TABLE path(path_id INTEGER PRIMARY KEY,text TEXT NOT NULL UNIQUE);
                CREATE TABLE fact(path_id INTEGER NOT NULL,ordinal INTEGER NOT NULL,
                                  PRIMARY KEY(path_id,ordinal)) WITHOUT ROWID;
                """
            )
            db.executemany(
                "INSERT INTO path VALUES (?,?)", enumerate(paths, 1)
            )
            db.executemany(
                "INSERT INTO fact VALUES (?,?)",
                (
                    (path_id, ordinal)
                    for path_id in range(1, len(paths) + 1)
                    for ordinal in range(references)
                ),
            )
            sql = "SELECT count(*) FROM path WHERE text GLOB ?"
            params = ("v6/*",)
        elif cell == "segments":
            segments = sorted({part for path in paths for part in path.split("/")})
            segment_id = {part: ix + 1 for ix, part in enumerate(segments)}
            db.executescript(
                """
                CREATE TABLE segment(segment_id INTEGER PRIMARY KEY,text TEXT NOT NULL UNIQUE);
                CREATE TABLE path(path_id INTEGER PRIMARY KEY);
                CREATE TABLE path_segment(
                  path_id INTEGER NOT NULL,ordinal INTEGER NOT NULL,segment_id INTEGER NOT NULL,
                  PRIMARY KEY(path_id,ordinal)
                ) WITHOUT ROWID;
                CREATE INDEX path_segment_value ON path_segment(segment_id,path_id,ordinal);
                CREATE TABLE fact(path_id INTEGER NOT NULL,ordinal INTEGER NOT NULL,
                                  PRIMARY KEY(path_id,ordinal)) WITHOUT ROWID;
                """
            )
            db.executemany("INSERT INTO segment VALUES (?,?)", enumerate(segments, 1))
            db.executemany("INSERT INTO path VALUES (?)", ((ix,) for ix in range(1, len(paths) + 1)))
            db.executemany(
                "INSERT INTO path_segment VALUES (?,?,?)",
                (
                    (path_id, ordinal, segment_id[part])
                    for path_id, path in enumerate(paths, 1)
                    for ordinal, part in enumerate(path.split("/"))
                ),
            )
            db.executemany(
                "INSERT INTO fact VALUES (?,?)",
                (
                    (path_id, ordinal)
                    for path_id in range(1, len(paths) + 1)
                    for ordinal in range(references)
                ),
            )
            sql = """
              SELECT count(DISTINCT ps.path_id)
              FROM path_segment ps JOIN segment s ON s.segment_id=ps.segment_id
              WHERE ps.ordinal=0 AND s.text=?
            """
            params = ("v6",)
        else:
            db.executescript(
                """
                CREATE TABLE fact(path TEXT NOT NULL,ordinal INTEGER NOT NULL,
                                  PRIMARY KEY(path,ordinal)) WITHOUT ROWID;
                """
            )
            db.executemany(
                "INSERT INTO fact VALUES (?,?)",
                ((path, ordinal) for path in paths for ordinal in range(references)),
            )
            sql = "SELECT count(DISTINCT path) FROM fact WHERE path GLOB ?"
            params = ("v6/*",)
        db.commit()
        ingest_ms = (time.perf_counter() - started) * 1000
        started = time.perf_counter()
        for _ in range(100):
            db.execute(sql, params).fetchone()
        filter_ms = (time.perf_counter() - started) * 10
        result = {
            "cell": cell,
            "paths": len(paths),
            "references": len(paths) * references,
            "db_bytes": db_bytes(db),
            "bytes_per_reference": db_bytes(db) / (len(paths) * references),
            "ingest_ms": ingest_ms,
            "filter_ms": filter_ms,
            "plan": [
                row[3]
                for row in db.execute(
                    "EXPLAIN QUERY PLAN " + sql, params
                ).fetchall()
            ],
        }
        db.close()
        return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default="../..")
    parser.add_argument("--references", type=int, default=20)
    parser.add_argument("--output", default="bench/file_span/path-results.json")
    args = parser.parse_args()
    root = Path(args.root).resolve()
    paths = subprocess.check_output(["git", "ls-files"], cwd=root, text=True).splitlines()
    results = [
        measure(paths, args.references, cell)
        for cell in ("whole", "segments", "repeated")
    ]
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")
    print(json.dumps(results, sort_keys=True))
