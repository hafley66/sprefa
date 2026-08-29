#!/usr/bin/env python3
"""Entrypoint crawl of a Rust corpus over sprefa-extract's resolved_edge plane.

Inputs, all produced by the battery scripts in the lane scratch:
  defs.jsonl          one row per call-plane def node (path, name, kind, span)
  resolve_edges.jsonl the union of the per-crate `extract --resolve` runs
  sites.jsonl         one row per call site (used only for the report's denominators)

A node is (path, name). BFS runs from two disjoint root sets: the program roots
(fn main in every src/bin/*.rs, plus every `pub(crate) fn handle_*` under
crates/rust-analyzer/src/handlers/) and the test roots (every fn carrying a
`#[test]` attribute). Depth is the number of resolved_edge hops from the
nearest root.

Usage: rust.crawl.py <scratch-dir> <corpus-root>
"""
import json, os, sys, re, collections

SCRATCH = sys.argv[1] if len(sys.argv) > 1 else "."
CORPUS = (sys.argv[2] if len(sys.argv) > 2 else ".").rstrip("/") + "/"

# ---------------------------------------------------------------- load defs
defs = []
for line in open(os.path.join(SCRATCH, "defs.jsonl")):
    d = json.loads(line)
    if "/test_data/" in d["path"] or not d["name"]:
        continue
    defs.append(d)

# A def is addressed by (path, name); a name declared twice in one file
# collapses, matching how resolved_edge names its endpoints.
by_key = collections.defaultdict(list)
for d in defs:
    by_key[(d["path"], d["name"])].append(d)
span_of = {k: max(v["end"] - v["start"] for v in vs) for k, vs in by_key.items()}
all_nodes = set(by_key)

# ---------------------------------------------------------------- load edges
out_edges = collections.defaultdict(set)
in_edges = collections.defaultdict(set)
edge_count = 0
for line in open(os.path.join(SCRATCH, "resolve_edges.jsonl")):
    o = json.loads(line)
    if o["record"] != "resolved_edge":
        continue
    src = (o["caller_path"].replace(CORPUS, ""), o["caller_name"])
    dst = (o["callee_path"].replace(CORPUS, ""), o["callee_name"])
    out_edges[src].add(dst)
    in_edges[dst].add(src)
    edge_count += 1

# ---------------------------------------------------------------- roots
def defs_in(path):
    return sorted(((p, n) for (p, n) in all_nodes if p == path), key=lambda k: k[1])

program_roots = set()
for path, _ in list(all_nodes):
    if re.search(r"/src/bin/[^/]+\.rs$", path):
        if (path, "main") in all_nodes:
            program_roots.add((path, "main"))
handlers_dir = "crates/rust-analyzer/src/handlers/"
handler_names = []
for path in sorted({p for p, _ in all_nodes if p.startswith(handlers_dir)}):
    src = open(CORPUS + path, encoding="utf-8", errors="replace").read()
    for m in re.finditer(r"pub\(crate\) fn (handle_[A-Za-z0-9_]+)", src):
        name = m.group(1)
        if (path, name) in all_nodes:
            program_roots.add((path, name))
            handler_names.append(name)

test_roots = set()
TEST_ATTR = re.compile(rb"#\[test\]")
for path in sorted({p for p, _ in all_nodes}):
    raw = open(CORPUS + path, "rb").read()
    marks = [m.end() for m in TEST_ATTR.finditer(raw)]
    if not marks:
        continue
    cands = sorted((d["start"], d["name"]) for d in defs if d["path"] == path)
    for mark in marks:
        nxt = next((n for s, n in cands if s >= mark), None)
        if nxt:
            test_roots.add((path, nxt))

# ---------------------------------------------------------------- bfs
def bfs(roots):
    depth = {r: 0 for r in roots if r in all_nodes}
    frontier = list(depth)
    d = 0
    while frontier:
        d += 1
        nxt = []
        for node in frontier:
            for dst in out_edges.get(node, ()):
                if dst not in depth and dst in all_nodes:
                    depth[dst] = d
                    nxt.append(dst)
        frontier = nxt
    return depth

prog_depth = bfs(program_roots)
test_depth = bfs(test_roots)
both = set(prog_depth) | set(test_depth)

# ---------------------------------------------------------------- report
def hist(depth):
    h = collections.Counter(depth.values())
    return [(d, h[d]) for d in sorted(h)]

report = {
    "defs_total": len(all_nodes),
    "edges_total": edge_count,
    "program_roots": len(program_roots),
    "program_handlers": len(set(handler_names)),
    "test_roots": len(test_roots),
    "program_reachable": len(prog_depth),
    "test_reachable": len(test_depth),
    "union_reachable": len(both),
    "program_depth_hist": hist(prog_depth),
    "test_depth_hist": hist(test_depth),
}
unreachable = sorted(all_nodes - both, key=lambda k: -span_of[k])
report["unreachable_total"] = len(unreachable)
report["unreachable_top20"] = [
    {"path": p, "name": n, "span_bytes": span_of[(p, n)],
     "in_edges": len(in_edges.get((p, n), ())),
     "out_edges": len(out_edges.get((p, n), ()))}
    for p, n in unreachable[:20]
]
report["out_degree_top20"] = [
    {"path": p, "name": n, "out_degree": len(v),
     "depth": prog_depth.get((p, n), test_depth.get((p, n), -1))}
    for (p, n), v in sorted(out_edges.items(), key=lambda kv: -len(kv[1]))[:20]
    if (p, n) in all_nodes
][:20]

json.dump(report, open(os.path.join(SCRATCH, "crawl.json"), "w"), indent=1)
for k in ("defs_total", "edges_total", "program_roots", "program_handlers",
          "test_roots", "program_reachable", "test_reachable",
          "union_reachable", "unreachable_total"):
    print(f"{k:22s} {report[k]}")
print("program depth hist", report["program_depth_hist"])
print("test depth hist   ", report["test_depth_hist"])
