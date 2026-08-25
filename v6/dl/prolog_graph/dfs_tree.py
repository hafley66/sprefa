#!/usr/bin/env python3
"""DFS preorder from the entry over reach_edge rows (children in byte-offset order).

usage: dl6 run import_graph.dl6 --in-memory --root <repo> --arrive "want=v6/prolog/*.pl" \
         --arrive "want=v6/prolog/**/*.pl" --arrive "entry=v6/prolog/compile.pl" \
         --final-only --final-tsv --final-rels reach_edge,dep_count | dfs_tree.py v6/prolog/compile.pl
"""
import sys
entry = sys.argv[1]
edges, deps = {}, {}
for line in sys.stdin:
    f = line.rstrip("\n").split("\t")
    if f[0] == "reach_edge":
        edges.setdefault(f[1], []).append((int(f[2]), f[3]))
    elif f[0] == "dep_count":
        deps[f[1]] = int(f[2])
seen = set()
def walk(path, depth):
    if path in seen:
        return
    seen.add(path)
    print("  " * depth + path, f"deps={deps.get(path, 0)}")
    for _, child in sorted(edges.get(path, [])):
        walk(child, depth + 1)
walk(entry, 0)
