#!/usr/bin/env python3
"""Content-source leg for BlobSpan text/line/column bindings."""

from __future__ import annotations

import argparse
import collections
import json
import os
import resource
import sqlite3
import subprocess
import sys
import tempfile
import time
from array import array
from pathlib import Path


class GitBatch:
    def __init__(self, root):
        self.proc = subprocess.Popen(
            ["git", "cat-file", "--batch"],
            cwd=root,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def read(self, oid):
        self.proc.stdin.write(oid.encode() + b"\n")
        self.proc.stdin.flush()
        header = self.proc.stdout.readline().decode().strip().split()
        if len(header) != 3:
            raise RuntimeError(header)
        size = int(header[2])
        content = self.proc.stdout.read(size)
        if self.proc.stdout.read(1) != b"\n":
            raise RuntimeError("git batch response missing delimiter")
        return content

    def close(self):
        self.proc.stdin.close()
        self.proc.wait(timeout=10)


def tracked_blobs(root, limit):
    output = subprocess.check_output(
        [
            "git",
            "ls-tree",
            "-r",
            "--format=%(objecttype) %(objectname) %(path)",
            "HEAD",
        ],
        cwd=root,
        text=True,
    )
    rows = []
    seen = set()
    for line in output.splitlines():
        object_type, oid, path = line.split(" ", 2)
        if object_type != "blob":
            continue
        if oid in seen:
            continue
        seen.add(oid)
        rows.append((oid, path))
        if len(rows) == limit:
            break
    return rows


def timed_git(root, oids, rounds):
    batch = GitBatch(root)
    byte_count = 0
    started = time.perf_counter()
    for _ in range(rounds):
        for oid in oids:
            byte_count += len(batch.read(oid))
    elapsed = time.perf_counter() - started
    batch.close()
    return elapsed * 1000, byte_count


def timed_sqlite(contents, rounds):
    with tempfile.TemporaryDirectory(prefix="file-content-lab-") as temp:
        path = Path(temp) / "content.sqlite"
        db = sqlite3.connect(path)
        db.execute("PRAGMA journal_mode=OFF")
        db.execute("PRAGMA synchronous=OFF")
        db.execute("CREATE TABLE stored_blob(blob_id INTEGER PRIMARY KEY,content BLOB NOT NULL)")
        db.executemany(
            "INSERT INTO stored_blob VALUES (?,?)",
            ((ix + 1, content) for ix, content in enumerate(contents)),
        )
        db.commit()
        byte_count = 0
        started = time.perf_counter()
        for _ in range(rounds):
            for blob_id in range(1, len(contents) + 1):
                byte_count += len(
                    db.execute(
                        "SELECT content FROM stored_blob WHERE blob_id=?", (blob_id,)
                    ).fetchone()[0]
                )
        elapsed = time.perf_counter() - started
        page_count = db.execute("PRAGMA page_count").fetchone()[0]
        page_size = db.execute("PRAGMA page_size").fetchone()[0]
        plan = db.execute(
            "EXPLAIN QUERY PLAN SELECT content FROM stored_blob WHERE blob_id=?", (1,)
        ).fetchone()[3]
        db.close()
    return elapsed * 1000, byte_count, page_count * page_size, plan


def bounded_indexes(contents, cache_bytes):
    cache = collections.OrderedDict()
    held = 0
    peak = 0
    index_bytes = 0
    for blob_id, content in enumerate(contents, 1):
        offsets = array("I", [0])
        offsets.extend(ix + 1 for ix, byte in enumerate(content) if byte == 10)
        index_bytes += len(offsets) * offsets.itemsize
        if len(content) <= cache_bytes:
            while cache and held + len(content) > cache_bytes:
                _, evicted = cache.popitem(last=False)
                held -= len(evicted)
            cache[blob_id] = content
            held += len(content)
            peak = max(peak, held)
    return index_bytes, peak, len(cache)


def main(args):
    root = Path(args.root).resolve()
    blobs = tracked_blobs(root, args.blobs)
    loader = GitBatch(root)
    contents = [loader.read(oid) for oid, _ in blobs]
    loader.close()
    oids = [oid for oid, _ in blobs]

    git_ms, git_bytes = timed_git(root, oids, args.rounds)
    sqlite_ms, sqlite_bytes, sqlite_db_bytes, sqlite_plan = timed_sqlite(
        contents, args.rounds
    )
    newline_index_bytes, cache_peak_bytes, cached_blobs = bounded_indexes(
        contents, args.cache_bytes
    )
    max_rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    if sys.platform != "darwin":
        max_rss *= 1024
    result = {
        "tracked_blobs": len(blobs),
        "rounds": args.rounds,
        "source_bytes": sum(map(len, contents)),
        "git_cat_file_batch_ms": git_ms,
        "git_bytes_read": git_bytes,
        "sqlite_read_ms": sqlite_ms,
        "sqlite_bytes_read": sqlite_bytes,
        "sqlite_db_bytes": sqlite_db_bytes,
        "sqlite_plan": sqlite_plan,
        "newline_index_bytes": newline_index_bytes,
        "cache_limit_bytes": args.cache_bytes,
        "cache_peak_bytes": cache_peak_bytes,
        "cached_blobs": cached_blobs,
        "process_peak_rss_bytes": max_rss,
    }
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default="../..")
    parser.add_argument("--blobs", type=int, default=300)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--cache-bytes", type=int, default=16 * 1024 * 1024)
    parser.add_argument("--output", default="bench/file_span/content-results.json")
    main(parser.parse_args())
