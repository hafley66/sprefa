#!/usr/bin/env python3
"""Convert madge --json output (paths relative to the madge root dir) to normal-form tsv.

usage: madge2tsv.py <prefix> <out.tsv> < madge.json
prefix is prepended to each path (e.g. src when madge ran on <corpus>/src).
Columns: src_path src_name dst_path dst_name (names empty for module family).
Skips dependency strings that do not look like resolved files (no .ts suffix).
"""
import json, sys

prefix, out = sys.argv[1], sys.argv[2]
suffixes = (".ts", ".tsx", ".d.ts", ".cts", ".d.cts", ".mts")
data = json.load(sys.stdin)
rows = 0
with open(out, "w") as f:
    for src, deps in data.items():
        s = f"{prefix}/{src}"
        for d in deps:
            if not d.endswith(suffixes):
                continue
            f.write(f"{s}\t\t{prefix}/{d}\t\n")
            rows += 1
print(rows)
