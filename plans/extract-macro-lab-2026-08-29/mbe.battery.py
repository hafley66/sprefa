#!/usr/bin/env python3
"""Regenerates defs.jsonl/resolve_edges.jsonl for rust.crawl.py with the mbe
binary, and counts total call sites across the same 873-file bucket
(plans/extract-macro-lab-2026-08-29/PLAN.md's own bucket, crates/*/src/**).

Usage: mbe.battery.py <extract-binary> <corpus-root> <scratch-dir>
"""
import json, os, subprocess, sys, concurrent.futures as cf

EXTRACT = sys.argv[1]
CORPUS = sys.argv[2].rstrip("/") + "/"
SCRATCH = sys.argv[3]
os.makedirs(SCRATCH, exist_ok=True)

out = subprocess.run(["find", "crates", "-name", "*.rs"], cwd=CORPUS, capture_output=True, text=True).stdout
files = sorted({f for f in out.splitlines() if "/src/" in f})
print("files", len(files))


def run_file(rel):
    r = subprocess.run(
        ["timeout", "10", EXTRACT, "--family", "call", CORPUS + rel],
        capture_output=True, text=True,
    )
    defs, sites = [], 0
    for line in r.stdout.splitlines():
        o = json.loads(line)
        if o.get("record") == "node":
            defs.append({"path": rel, "name": o.get("name"), "start": o["span"]["start"], "end": o["span"]["end"]})
        elif o.get("record") == "site":
            sites += 1
    return rel, r.returncode, defs, sites


all_defs, total_sites, bad = [], 0, []
with cf.ThreadPoolExecutor(max_workers=8) as ex:
    for rel, rc, defs, sites in ex.map(run_file, files):
        if rc != 0:
            bad.append((rel, rc))
        all_defs.extend(defs)
        total_sites += sites

with open(os.path.join(SCRATCH, "defs.jsonl"), "w") as f:
    for d in all_defs:
        f.write(json.dumps(d) + "\n")

print("defs", len(all_defs), "sites", total_sites, "rc_nonzero", len(bad))
if bad:
    print("rc_nonzero sample", bad[:10])

crates = sorted({rel.split("/")[1] for rel in files})
resolve_edges = 0
resolve_bad = []
with open(os.path.join(SCRATCH, "resolve_edges.jsonl"), "w") as out_f:
    for crate in crates:
        crate_files = [CORPUS + rel for rel in files if rel.startswith(f"crates/{crate}/")]
        if not crate_files:
            continue
        r = subprocess.run(
            ["timeout", "10", EXTRACT, "--resolve", "--family", "call", *crate_files],
            capture_output=True, text=True,
        )
        if r.returncode != 0:
            resolve_bad.append((crate, r.returncode, r.stderr[:200]))
            continue
        for line in r.stdout.splitlines():
            o = json.loads(line)
            if o.get("record") == "resolved_edge":
                out_f.write(line + "\n")
                resolve_edges += 1

print("resolve_edges", resolve_edges, "crates", len(crates), "resolve_bad", len(resolve_bad))
if resolve_bad:
    print("resolve_bad", resolve_bad[:10])
