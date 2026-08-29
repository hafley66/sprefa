#!/usr/bin/env python3
"""Per-file `--family call`, path injected into each row. argv: LIST OUT_JSONL OUT_TSV"""
import json, os, subprocess, sys, time
from concurrent.futures import ThreadPoolExecutor

EX = "/Users/chrishafley/projects/sprefa/.boop-worktrees/crawl/extract-typescript-5/v6/sprefa-extract/target/release/extract"
CORPUS = "/Users/chrishafley/projects/TypeScript-5.9"


def run_one(rel):
    t0 = time.monotonic()
    try:
        p = subprocess.run(
            [EX, "--family", "call", os.path.join(CORPUS, rel)],
            capture_output=True, timeout=10)
        rc, out = p.returncode, p.stdout
    except subprocess.TimeoutExpired as e:
        rc, out = 124, e.stdout or b""
    ms = int((time.monotonic() - t0) * 1000)
    rows = []
    counts = {}
    for line in out.split(b"\n"):
        if not line:
            continue
        o = json.loads(line)
        o["path"] = rel
        counts[o["record"]] = counts.get(o["record"], 0) + 1
        rows.append(json.dumps(o, separators=(",", ":")))
    return rel, rc, ms, rows, counts


files = [l.strip() for l in open(sys.argv[1]) if l.strip()]
with open(sys.argv[2], "w") as fj, open(sys.argv[3], "w") as ft:
    ft.write("path\trc\tms\tsite\tnode\tedge\tsig\tspecifier\tunresolved\n")
    with ThreadPoolExecutor(max_workers=8) as ex:
        for rel, rc, ms, rows, c in ex.map(run_one, files):
            fj.write("\n".join(rows) + ("\n" if rows else ""))
            ft.write("\t".join(str(x) for x in [
                rel, rc, ms, c.get("site", 0), c.get("node", 0), c.get("edge", 0),
                c.get("sig", 0), c.get("specifier", 0), c.get("unresolved", 0)]) + "\n")
print("DONE", len(files), flush=True)
