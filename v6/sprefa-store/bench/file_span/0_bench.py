#!/usr/bin/env python3
"""Deterministic null-free file/span storage lab.

The default driver runs each cell in a fresh child process so peak RSS is
per-cell. Database files live in a temporary directory and only compact JSON
measurements are retained.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import resource
import sqlite3
import subprocess
import sys
import tempfile
import time
from pathlib import Path

CELLS = (
    "span_ref",
    "embedded",
    "located_ref",
    "located_inline",
    "text_baseline",
)


def batches(rows, size=10_000):
    batch = []
    for row in rows:
        batch.append(row)
        if len(batch) == size:
            yield batch
            batch = []
    if batch:
        yield batch


def insert_many(db, sql, rows):
    statements = 0
    for batch in batches(rows):
        db.executemany(sql, batch)
        statements += 1
    return statements


def digest16(value):
    return hashlib.blake2b(str(value).encode(), digest_size=16).digest()


def schema_common(db):
    db.executescript(
        """
        CREATE TABLE repo(
          repo_id INTEGER PRIMARY KEY,
          name_id INTEGER NOT NULL
        );
        CREATE TABLE path(
          path_id INTEGER PRIMARY KEY,
          normalized_path TEXT NOT NULL UNIQUE
        );
        CREATE TABLE file(
          file_id INTEGER PRIMARY KEY,
          repo_id INTEGER NOT NULL,
          path_id INTEGER NOT NULL,
          UNIQUE(repo_id,path_id)
        );
        CREATE TABLE committed_rev(
          rev_id INTEGER PRIMARY KEY,
          repo_id INTEGER NOT NULL,
          git_oid BLOB NOT NULL,
          UNIQUE(repo_id,git_oid)
        );
        CREATE TABLE work_rev(
          rev_id INTEGER PRIMARY KEY,
          repo_id INTEGER NOT NULL,
          root_id INTEGER NOT NULL,
          base_rev_id INTEGER NOT NULL,
          UNIQUE(root_id)
        );
        CREATE VIEW rev_member AS
          SELECT rev_id,repo_id FROM committed_rev
          UNION ALL
          SELECT rev_id,repo_id FROM work_rev;
        CREATE TABLE blob(
          blob_id INTEGER PRIMARY KEY,
          digest BLOB NOT NULL UNIQUE,
          byte_len INTEGER NOT NULL,
          line_count INTEGER NOT NULL
        );
        CREATE TABLE git_blob(
          blob_id INTEGER NOT NULL,
          repo_id INTEGER NOT NULL,
          git_oid BLOB NOT NULL,
          PRIMARY KEY(blob_id,repo_id)
        ) WITHOUT ROWID;
        CREATE TABLE stored_blob(
          blob_id INTEGER PRIMARY KEY,
          content BLOB NOT NULL
        );
        CREATE VIEW available_blob AS
          SELECT blob_id FROM git_blob
          UNION
          SELECT blob_id FROM stored_blob;
        CREATE TABLE rev_file(
          rev_file_id INTEGER PRIMARY KEY,
          rev_id INTEGER NOT NULL,
          file_id INTEGER NOT NULL,
          blob_id INTEGER NOT NULL,
          UNIQUE(rev_id,file_id)
        );
        CREATE INDEX rev_file_blob ON rev_file(blob_id);
        CREATE TABLE name(
          name_id INTEGER PRIMARY KEY,
          text TEXT NOT NULL UNIQUE
        );
        """
    )


def schema_cell(db, cell):
    if cell == "span_ref":
        db.executescript(
            """
            CREATE TABLE blob_span(
              blob_span_id INTEGER PRIMARY KEY,
              blob_id INTEGER NOT NULL,
              start INTEGER NOT NULL,
              end INTEGER NOT NULL,
              UNIQUE(blob_id,start,end)
            );
            CREATE TABLE fact(
              rev_file_id INTEGER NOT NULL,
              blob_span_id INTEGER NOT NULL,
              family INTEGER NOT NULL,
              kind INTEGER NOT NULL,
              name_id INTEGER NOT NULL,
              PRIMARY KEY(rev_file_id,blob_span_id,family,kind)
            ) WITHOUT ROWID;
            CREATE INDEX fact_span ON fact(blob_span_id);
            """
        )
    elif cell == "embedded":
        db.executescript(
            """
            CREATE TABLE fact(
              rev_file_id INTEGER NOT NULL,
              blob_id INTEGER NOT NULL,
              start INTEGER NOT NULL,
              end INTEGER NOT NULL,
              family INTEGER NOT NULL,
              kind INTEGER NOT NULL,
              name_id INTEGER NOT NULL,
              PRIMARY KEY(rev_file_id,start,end,family,kind)
            ) WITHOUT ROWID;
            CREATE INDEX fact_span ON fact(blob_id,start,end);
            """
        )
    elif cell == "located_ref":
        db.executescript(
            """
            CREATE TABLE blob_span(
              blob_span_id INTEGER PRIMARY KEY,
              blob_id INTEGER NOT NULL,
              start INTEGER NOT NULL,
              end INTEGER NOT NULL,
              UNIQUE(blob_id,start,end)
            );
            CREATE TABLE file_span(
              file_span_id INTEGER PRIMARY KEY,
              rev_file_id INTEGER NOT NULL,
              blob_span_id INTEGER NOT NULL,
              UNIQUE(rev_file_id,blob_span_id)
            );
            CREATE INDEX file_span_span ON file_span(blob_span_id);
            CREATE TABLE fact(
              file_span_id INTEGER NOT NULL,
              family INTEGER NOT NULL,
              kind INTEGER NOT NULL,
              name_id INTEGER NOT NULL,
              PRIMARY KEY(file_span_id,family,kind)
            ) WITHOUT ROWID;
            """
        )
    elif cell == "located_inline":
        db.executescript(
            """
            CREATE TABLE file_span(
              file_span_id INTEGER PRIMARY KEY,
              rev_file_id INTEGER NOT NULL,
              start INTEGER NOT NULL,
              end INTEGER NOT NULL,
              UNIQUE(rev_file_id,start,end)
            );
            CREATE TABLE fact(
              file_span_id INTEGER NOT NULL,
              family INTEGER NOT NULL,
              kind INTEGER NOT NULL,
              name_id INTEGER NOT NULL,
              PRIMARY KEY(file_span_id,family,kind)
            ) WITHOUT ROWID;
            """
        )
    elif cell == "text_baseline":
        db.executescript(
            """
            CREATE TABLE fact_text(
              repo TEXT NOT NULL,
              rev TEXT NOT NULL,
              path TEXT NOT NULL,
              digest TEXT NOT NULL,
              start INTEGER NOT NULL,
              end INTEGER NOT NULL,
              family TEXT NOT NULL,
              kind TEXT NOT NULL,
              name TEXT NOT NULL,
              PRIMARY KEY(repo,rev,path,start,end,family,kind)
            ) WITHOUT ROWID;
            CREATE INDEX fact_text_span ON fact_text(digest,start,end);
            """
        )
    else:
        raise ValueError(cell)


def model(args):
    repo_rows = [(repo, repo) for repo in range(1, args.repos + 1)]
    name_rows = [(name, f"name_{name:05d}") for name in range(1, args.names + 1)]
    path_rows = [
        (
            local_file + 1,
            f"packages/p{local_file % 37:02d}/src/d{local_file % 19:02d}/file_{local_file:05d}.ts",
        )
        for local_file in range(args.files_per_repo)
    ]
    file_rows = []
    committed_rows = []
    work_rows = []
    blob_rows = []
    git_blob_rows = []
    stored_blob_rows = []
    rev_file_rows = []
    file_blob_by_rev = {}
    next_blob = 1
    next_file = 1
    next_rev_file = 1

    for repo in range(1, args.repos + 1):
        rev_ids = []
        for rev_ix in range(args.revs - 1):
            rev_id = (repo - 1) * args.revs + rev_ix + 1
            rev_ids.append(rev_id)
            committed_rows.append((rev_id, repo, hashlib.sha1(f"{repo}:{rev_ix}".encode()).digest()))
        work_rev_id = (repo - 1) * args.revs + args.revs
        rev_ids.append(work_rev_id)
        work_rows.append((work_rev_id, repo, repo, rev_ids[-2]))

        for local_file in range(args.files_per_repo):
            path_id = local_file + 1
            file_id = next_file
            next_file += 1
            file_rows.append((file_id, repo, path_id))
            base_blob = next_blob
            next_blob += 1
            blob_rows.append((base_blob, digest16(("base", repo, local_file)), args.blob_bytes, args.lines))
            git_blob_rows.append((base_blob, repo, hashlib.sha1(f"b:{repo}:{local_file}".encode()).digest()))
            if local_file % args.store_every == 0:
                stored_blob_rows.append((base_blob, bytes(args.blob_bytes)))

            current_blob = base_blob
            for rev_ix, rev_id in enumerate(rev_ids):
                if rev_ix and (local_file + rev_ix) % args.change_every == 0:
                    current_blob = next_blob
                    next_blob += 1
                    blob_rows.append(
                        (current_blob, digest16(("change", repo, local_file, rev_ix)), args.blob_bytes, args.lines)
                    )
                    if rev_ix < args.revs - 1:
                        git_blob_rows.append(
                            (current_blob, repo, hashlib.sha1(f"c:{repo}:{local_file}:{rev_ix}".encode()).digest())
                        )
                    if local_file % args.store_every == 0:
                        stored_blob_rows.append((current_blob, bytes(args.blob_bytes)))
                rev_file_rows.append((next_rev_file, rev_id, file_id, current_blob))
                file_blob_by_rev[next_rev_file] = (
                    repo,
                    rev_id,
                    path_rows[local_file][1],
                    current_blob,
                )
                next_rev_file += 1

    return {
        "repo": repo_rows,
        "name": name_rows,
        "path": path_rows,
        "file": file_rows,
        "committed": committed_rows,
        "work": work_rows,
        "blob": blob_rows,
        "git_blob": git_blob_rows,
        "stored_blob": stored_blob_rows,
        "rev_file": rev_file_rows,
        "file_blob_by_rev": file_blob_by_rev,
    }


def content_spans(blob_rows, spans_per_blob, blob_bytes):
    width = max(1, blob_bytes // (spans_per_blob + 1))
    for blob_id, _, _, _ in blob_rows:
        for slot in range(spans_per_blob):
            start = slot * width
            yield (blob_id, slot, start, min(blob_bytes, start + width // 2 + 1))


def location_multiplicities(args, locations):
    if not args.census:
        return [args.families] * locations
    census = json.loads(Path(args.census).read_text())
    histogram = {}
    for multiplicity, count in census["multiplicity_histogram"].items():
        capped = min(int(multiplicity), args.max_multiplicity)
        histogram[capped] = histogram.get(capped, 0) + count
    total = sum(histogram.values())
    values = []
    for multiplicity, count in sorted(histogram.items()):
        values.extend([multiplicity] * (locations * count // total))
    values.extend([max(histogram)] * (locations - len(values)))
    return values


def ingest_cell(db, cell, data, args):
    statements = 0
    statements += insert_many(db, "INSERT INTO repo VALUES (?,?)", data["repo"])
    statements += insert_many(db, "INSERT INTO name VALUES (?,?)", data["name"])
    statements += insert_many(db, "INSERT INTO path VALUES (?,?)", data["path"])
    statements += insert_many(db, "INSERT INTO file VALUES (?,?,?)", data["file"])
    statements += insert_many(db, "INSERT INTO committed_rev VALUES (?,?,?)", data["committed"])
    statements += insert_many(db, "INSERT INTO work_rev VALUES (?,?,?,?)", data["work"])
    statements += insert_many(db, "INSERT INTO blob VALUES (?,?,?,?)", data["blob"])
    statements += insert_many(db, "INSERT INTO git_blob VALUES (?,?,?)", data["git_blob"])
    statements += insert_many(db, "INSERT INTO stored_blob VALUES (?,?)", data["stored_blob"])
    statements += insert_many(db, "INSERT INTO rev_file VALUES (?,?,?,?)", data["rev_file"])

    span_rows = list(content_spans(data["blob"], args.spans_per_blob, args.blob_bytes))
    span_id = {(blob, slot): ix + 1 for ix, (blob, slot, _, _) in enumerate(span_rows)}
    location_count = len(data["rev_file"]) * args.spans_per_blob
    multiplicities = location_multiplicities(args, location_count)

    def located():
        location = 0
        for rev_file_id, _, _, blob in data["rev_file"]:
            for slot in range(args.spans_per_blob):
                yield rev_file_id, blob, slot, multiplicities[location]
                location += 1

    if cell in ("span_ref", "located_ref"):
        statements += insert_many(
            db,
            "INSERT INTO blob_span VALUES (?,?,?,?)",
            ((ix + 1, blob, start, end) for ix, (blob, _, start, end) in enumerate(span_rows)),
        )

    if cell == "span_ref":
        facts = (
            (
                rev_file_id,
                span_id[(blob, slot)],
                reference % 8,
                reference // 8,
                (blob + slot) % args.names + 1,
            )
            for rev_file_id, blob, slot, multiplicity in located()
            for reference in range(multiplicity)
        )
        statements += insert_many(db, "INSERT INTO fact VALUES (?,?,?,?,?)", facts)
    elif cell == "embedded":
        span_coord = {(blob, slot): (start, end) for blob, slot, start, end in span_rows}
        facts = (
            (
                rev_file_id,
                blob,
                span_coord[(blob, slot)][0],
                span_coord[(blob, slot)][1],
                reference % 8,
                reference // 8,
                (blob + slot) % args.names + 1,
            )
            for rev_file_id, blob, slot, multiplicity in located()
            for reference in range(multiplicity)
        )
        statements += insert_many(db, "INSERT INTO fact VALUES (?,?,?,?,?,?,?)", facts)
    elif cell == "located_ref":
        file_span_rows = (
            (
                (rev_file_id - 1) * args.spans_per_blob + slot + 1,
                rev_file_id,
                span_id[(blob, slot)],
            )
            for rev_file_id, _, _, blob in data["rev_file"]
            for slot in range(args.spans_per_blob)
        )
        statements += insert_many(db, "INSERT INTO file_span VALUES (?,?,?)", file_span_rows)
        facts = (
            (
                (rev_file_id - 1) * args.spans_per_blob + slot + 1,
                reference % 8,
                reference // 8,
                (blob + slot) % args.names + 1,
            )
            for rev_file_id, blob, slot, multiplicity in located()
            for reference in range(multiplicity)
        )
        statements += insert_many(db, "INSERT INTO fact VALUES (?,?,?,?)", facts)
    elif cell == "located_inline":
        span_coord = {(blob, slot): (start, end) for blob, slot, start, end in span_rows}
        file_span_rows = (
            (
                (rev_file_id - 1) * args.spans_per_blob + slot + 1,
                rev_file_id,
                span_coord[(blob, slot)][0],
                span_coord[(blob, slot)][1],
            )
            for rev_file_id, blob, slot, _ in located()
        )
        statements += insert_many(db, "INSERT INTO file_span VALUES (?,?,?,?)", file_span_rows)
        facts = (
            (
                (rev_file_id - 1) * args.spans_per_blob + slot + 1,
                reference % 8,
                reference // 8,
                (blob + slot) % args.names + 1,
            )
            for rev_file_id, blob, slot, multiplicity in located()
            for reference in range(multiplicity)
        )
        statements += insert_many(db, "INSERT INTO fact VALUES (?,?,?,?)", facts)
    else:
        facts = (
            (
                f"repo_{repo:04d}",
                f"rev_{rev_id:08d}",
                path,
                digest16(blob).hex(),
                slot * max(1, args.blob_bytes // (args.spans_per_blob + 1)),
                slot * max(1, args.blob_bytes // (args.spans_per_blob + 1)) + 17,
                f"family_{reference % 8}",
                f"kind_{reference // 8}",
                f"name_{(blob + slot) % args.names + 1:05d}",
            )
            for rev_file_id, blob, slot, multiplicity in located()
            for repo, rev_id, path, _ in (data["file_blob_by_rev"][rev_file_id],)
            for reference in range(multiplicity)
        )
        statements += insert_many(db, "INSERT INTO fact_text VALUES (?,?,?,?,?,?,?,?,?)", facts)
    return statements, len(span_rows), sum(multiplicities), location_count


def timed_queries(db, cell, args):
    if cell == "span_ref":
        lookup = """
          SELECT count(*) FROM fact x
          JOIN rev_file rf ON rf.rev_file_id=x.rev_file_id
          JOIN file f ON f.file_id=rf.file_id
          JOIN path p ON p.path_id=f.path_id
          JOIN blob_span s ON s.blob_span_id=x.blob_span_id
          WHERE f.repo_id=? AND rf.rev_id=? AND p.normalized_path GLOB ?
        """
        reverse = """
          SELECT count(*) FROM fact x
          JOIN rev_file rf ON rf.rev_file_id=x.rev_file_id
          WHERE x.blob_span_id=?
        """
    elif cell == "embedded":
        lookup = """
          SELECT count(*) FROM fact x
          JOIN rev_file rf ON rf.rev_file_id=x.rev_file_id
          JOIN file f ON f.file_id=rf.file_id
          JOIN path p ON p.path_id=f.path_id
          WHERE f.repo_id=? AND rf.rev_id=? AND p.normalized_path GLOB ?
        """
        reverse = "SELECT count(*) FROM fact WHERE blob_id=? AND start=? AND end=?"
    elif cell == "located_ref":
        lookup = """
          SELECT count(*) FROM fact x
          JOIN file_span fs ON fs.file_span_id=x.file_span_id
          JOIN rev_file rf ON rf.rev_file_id=fs.rev_file_id
          JOIN file f ON f.file_id=rf.file_id
          JOIN path p ON p.path_id=f.path_id
          JOIN blob_span s ON s.blob_span_id=fs.blob_span_id
          WHERE f.repo_id=? AND rf.rev_id=? AND p.normalized_path GLOB ?
        """
        reverse = "SELECT count(*) FROM file_span WHERE blob_span_id=?"
    elif cell == "located_inline":
        lookup = """
          SELECT count(*) FROM fact x
          JOIN file_span fs ON fs.file_span_id=x.file_span_id
          JOIN rev_file rf ON rf.rev_file_id=fs.rev_file_id
          JOIN file f ON f.file_id=rf.file_id
          JOIN path p ON p.path_id=f.path_id
          WHERE f.repo_id=? AND rf.rev_id=? AND p.normalized_path GLOB ?
        """
        reverse = """
          SELECT count(*) FROM rev_file rf
          JOIN file_span fs ON fs.rev_file_id=rf.rev_file_id
          WHERE rf.blob_id=? AND fs.start=? AND fs.end=?
        """
    else:
        lookup = """
          SELECT count(*) FROM fact_text
          WHERE repo=? AND rev=? AND path GLOB ?
        """
        reverse = "SELECT count(*) FROM fact_text WHERE digest=? AND start=? AND end=?"

    repo = max(1, args.repos // 2)
    rev_id = (repo - 1) * args.revs + 1
    lookup_args = (
        (f"repo_{repo:04d}", f"rev_{rev_id:08d}", "packages/p01/*")
        if cell == "text_baseline"
        else (repo, rev_id, "packages/p01/*")
    )
    reverse_args = {
        "span_ref": (1,),
        "located_ref": (1,),
        "located_inline": (1, 0, max(1, args.blob_bytes // (args.spans_per_blob + 1)) // 2 + 1),
        "embedded": (1, 0, max(1, args.blob_bytes // (args.spans_per_blob + 1)) // 2 + 1),
        "text_baseline": (digest16(1).hex(), 0, 17),
    }[cell]

    query_times = {}
    for name, sql, params in (
        ("filter", lookup, lookup_args),
        ("reverse", reverse, reverse_args),
    ):
        started = time.perf_counter()
        for _ in range(args.query_repeats):
            db.execute(sql, params).fetchone()
        query_times[f"{name}_ms"] = (time.perf_counter() - started) * 1000 / args.query_repeats
        query_times[f"{name}_plan"] = [
            row[3] for row in db.execute("EXPLAIN QUERY PLAN " + sql, params).fetchall()
        ]
    return query_times


def dbstat_bytes(db):
    return {
        name: value
        for name, value in db.execute(
            "SELECT name,sum(pgsize) FROM dbstat GROUP BY name ORDER BY name"
        )
    }


def run_cell(args):
    db_path = Path(args.db)
    if db_path.exists():
        db_path.unlink()
    db = sqlite3.connect(db_path)
    db.execute("PRAGMA journal_mode=OFF")
    db.execute("PRAGMA synchronous=OFF")
    db.execute("PRAGMA temp_store=MEMORY")
    db.execute("PRAGMA cache_size=-32768")
    schema_common(db)
    schema_cell(db, args.cell)
    data = model(args)
    started = time.perf_counter()
    statements, distinct_spans, facts, located_spans = ingest_cell(
        db, args.cell, data, args
    )
    db.commit()
    ingest_ms = (time.perf_counter() - started) * 1000
    db.execute("ANALYZE")
    db.commit()
    query = timed_queries(db, args.cell, args)
    stats = dbstat_bytes(db)
    page_count = db.execute("PRAGMA page_count").fetchone()[0]
    page_size = db.execute("PRAGMA page_size").fetchone()[0]
    max_rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    if sys.platform != "darwin":
        max_rss *= 1024
    result = {
        "cell": args.cell,
        "repos": args.repos,
        "files": len(data["file"]),
        "revs": len(data["committed"]) + len(data["work"]),
        "rev_files": len(data["rev_file"]),
        "blobs": len(data["blob"]),
        "distinct_spans": distinct_spans,
        "located_spans": located_spans,
        "fact_rows": facts,
        "span_refs_per_located": facts / located_spans,
        "span_refs_per_distinct_content_span": facts / distinct_spans,
        "ingest_ms": ingest_ms,
        "batch_statements": statements,
        "db_bytes": page_count * page_size,
        "bytes_per_fact": page_count * page_size / facts,
        "peak_rss_bytes": max_rss,
        "dbstat": stats,
        **query,
    }
    print(json.dumps(result, sort_keys=True))


def run_driver(args):
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    rows = []
    with tempfile.TemporaryDirectory(prefix="file-span-lab-") as temp:
        for repeat in range(1, args.repeats + 1):
            for cell in CELLS:
                db_path = Path(temp) / f"{cell}-{repeat}.sqlite"
                cmd = [
                    sys.executable,
                    __file__,
                    "--cell",
                    cell,
                    "--db",
                    str(db_path),
                    "--repos",
                    str(args.repos),
                    "--files-per-repo",
                    str(args.files_per_repo),
                    "--revs",
                    str(args.revs),
                    "--spans-per-blob",
                    str(args.spans_per_blob),
                    "--families",
                    str(args.families),
                    "--names",
                    str(args.names),
                    "--blob-bytes",
                    str(args.blob_bytes),
                    "--lines",
                    str(args.lines),
                    "--change-every",
                    str(args.change_every),
                    "--store-every",
                    str(args.store_every),
                    "--query-repeats",
                    str(args.query_repeats),
                ]
                if args.census:
                    cmd.extend(
                        [
                            "--census",
                            str(Path(args.census).resolve()),
                            "--max-multiplicity",
                            str(args.max_multiplicity),
                        ]
                    )
                completed = subprocess.run(cmd, check=True, text=True, capture_output=True)
                row = json.loads(completed.stdout)
                row["repeat"] = repeat
                rows.append(row)
                print(
                    f"{cell} repeat={repeat} db={row['db_bytes']} "
                    f"bytes/fact={row['bytes_per_fact']:.2f} "
                    f"ingest_ms={row['ingest_ms']:.1f} rss={row['peak_rss_bytes']}"
                )
    output.write_text(json.dumps(rows, indent=2, sort_keys=True) + "\n")
    print(f"wrote {output}")


def parser():
    p = argparse.ArgumentParser()
    p.add_argument("--cell", choices=CELLS)
    p.add_argument("--db")
    p.add_argument("--output", default="bench/file_span/results.json")
    p.add_argument("--repeats", type=int, default=2)
    p.add_argument("--repos", type=int, default=12)
    p.add_argument("--files-per-repo", type=int, default=300)
    p.add_argument("--revs", type=int, default=4)
    p.add_argument("--spans-per-blob", type=int, default=8)
    p.add_argument("--families", type=int, default=3)
    p.add_argument("--names", type=int, default=500)
    p.add_argument("--blob-bytes", type=int, default=4096)
    p.add_argument("--lines", type=int, default=120)
    p.add_argument("--change-every", type=int, default=5)
    p.add_argument("--store-every", type=int, default=17)
    p.add_argument("--query-repeats", type=int, default=40)
    p.add_argument("--census")
    p.add_argument("--max-multiplicity", type=int, default=32)
    return p


if __name__ == "__main__":
    parsed = parser().parse_args()
    if parsed.cell:
        if not parsed.db:
            raise SystemExit("--db is required with --cell")
        run_cell(parsed)
    else:
        run_driver(parsed)
