#!/usr/bin/env python3
"""BFS over resolved_edge from the TypeScript 5.9 compiler entrypoints.

argv: CALL_JSONL RESOLVE_JSONL OUT_JSON
CALL_JSONL is per-file `--family call` output with a `path` field injected.
RESOLVE_JSONL is one whole-corpus `--resolve --family call,type` run.

A graph node is (path, name); `resolved_edge` names its endpoints that way and
carries no callee span, so same-name defs in one file fold into one node.
"""
import collections, json, os, sys

CALL, RESOLVE, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
PREFIX = "/Users/chrishafley/projects/TypeScript-5.9/"
DEF_KINDS = ("function", "method", "module")

nodes = {}
spans = collections.defaultdict(list)
sites = collections.defaultdict(list)
for line in open(CALL):
    o = json.loads(line)
    if o["record"] == "node":
        spans[o["path"]].append((o["span"]["start"], o["span"]["end"], o["kind"], o["name"]))
        if o["kind"] in DEF_KINDS and o["name"]:
            k = (o["path"], o["name"])
            a, b = o["span"]["start"], o["span"]["end"]
            if k not in nodes or b - a > nodes[k][1] - nodes[k][0]:
                nodes[k] = (a, b)
    elif o["record"] == "site":
        sites[o["path"]].append((o["span"]["start"], o["callee"]))

for p in spans:
    spans[p].sort()


def innermost(path, start):
    """Innermost node span containing `start`, or None."""
    best = None
    for a, b, k, n in spans.get(path, ()):
        if a > start:
            break
        if start < b and (best is None or a > best[0]):
            best = (a, b, k, n)
    return best


def named_owner(path, start):
    """Nearest enclosing function/method with a name, walking outward."""
    cands = [(a, b, k, n) for a, b, k, n in spans.get(path, ())
             if a <= start < b and k in DEF_KINDS and n]
    return (path, max(cands)[3]) if cands else None


out = collections.defaultdict(set)          # strict: the key the stream carries
out_folded = collections.defaultdict(set)   # closure@N folded into its named owner
closure_edges = 0
for line in open(RESOLVE):
    o = json.loads(line)
    if o["record"] != "resolved_edge":
        continue
    cpath = o["caller_path"].replace(PREFIX, "")
    src = (cpath, o["caller_name"])
    dst = (o["callee_path"].replace(PREFIX, ""), o["callee_name"])
    if dst not in nodes:
        continue
    out[src].add(dst)
    fsrc = src
    if (o["caller_name"] or "").startswith("closure@"):
        closure_edges += 1
        fsrc = named_owner(cpath, o["caller_site_start"]) or src
    out_folded[fsrc].add(dst)

ENTRY_FILES = ["src/tsc/tsc.ts", "src/tsserver/server.ts", "src/typescript/typescript.ts"]


def exported_functions(path):
    """`export function NAME` at column 0, matched against the def index."""
    text = open(PREFIX + path, encoding="utf8", errors="replace").read()
    names = set()
    for ln in text.split("\n"):
        if ln.startswith("export function "):
            names.add(ln[len("export function "):].split("(")[0].split("<")[0].strip())
    return {(path, n) for n in names if (path, n) in nodes}


def covering(path, start):
    return innermost(path, start)


def uncovered_callees(path):
    """Callees of sites in `path` that lie outside every def span, resolved by
    name when the name has exactly one def in the corpus."""
    byname = collections.defaultdict(list)
    for (p, n) in nodes:
        byname[n].append((p, n))
    got = set()
    for st, callee in sites.get(path, ()):
        if covering(path, st) is None and len(byname.get(callee, [])) == 1:
            got.add(byname[callee][0])
    return got


roots = {
    "A_strict": set().union(*[{k for k in nodes if k[0] == f} for f in ENTRY_FILES])
                | exported_functions("src/compiler/program.ts"),
    "B_testRunner": {k for k in nodes if k[0].startswith("src/testRunner/")},
}
roots["A_patched"] = roots["A_strict"] | set().union(
    *[uncovered_callees(f) for f in ENTRY_FILES])


def bfs(seed, graph):
    depth = {k: 0 for k in seed if k in nodes}
    q = collections.deque(depth)
    while q:
        u = q.popleft()
        for v in graph.get(u, ()):
            if v not in depth:
                depth[v] = depth[u] + 1
                q.append(v)
    return depth


plan = [("A_strict", roots["A_strict"], out),
        ("A_patched", roots["A_patched"], out),
        ("A_patched_closure_folded", roots["A_patched"], out_folded),
        ("B_testRunner", roots["B_testRunner"], out),
        ("B_testRunner_closure_folded", roots["B_testRunner"], out_folded)]

result = {"total_defs": len(nodes), "closure_caller_edges": closure_edges, "sets": {}}
depths = {}
for name, seed, graph in plan:
    d = bfs(seed, graph)
    depths[name] = d
    hist = collections.Counter(d.values())
    result["sets"][name] = {
        "seeds": len(seed),
        "seeds_in_graph": len([k for k in seed if k in nodes]),
        "reachable": len(d),
        "depth_hist": {str(k): hist[k] for k in sorted(hist)},
        "max_depth": max(hist) if hist else 0,
    }

union = set(depths["A_strict"]) | set(depths["B_testRunner"])
folded = set(depths["A_patched_closure_folded"]) | set(depths["B_testRunner_closure_folded"])
result["reachable_union_strict"] = len(union)
result["reachable_union_folded"] = len(folded)

unreach = sorted(((nodes[k][1] - nodes[k][0], k) for k in nodes if k not in union),
                 reverse=True)
result["largest_unreachable"] = [
    {"bytes": sz, "path": k[0], "name": k[1], "start": nodes[k][0],
     "reached_when_folded": k in folded} for sz, k in unreach[:20]]
result["highest_out_degree"] = [
    {"path": k[0], "name": k[1], "out": len(v)}
    for k, v in sorted(out.items(), key=lambda kv: -len(kv[1]))[:20] if k in nodes]

json.dump(result, open(OUT, "w"), indent=1)
for name, r in result["sets"].items():
    print(name, r["seeds_in_graph"], "seeds ->", r["reachable"], "of", len(nodes),
          "max depth", r["max_depth"])
print("union strict", len(union), "union closure-folded", len(folded), "of", len(nodes))
