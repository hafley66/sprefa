"""Entrypoint crawl over sprefa-extract Go resolved_edge JSONL.

Inputs:
  all_resolved.jsonl  --resolve dumps (one JSONL per package)
  defs.tsv            per-file call-plane node rows, prefixed with file path
Entrypoints: func main under cmd/, plus exported funcs of internal/compiler/program.go.
Second root set: functions defined in _test.go files.
"""
import json, sys, collections

SCRATCH = sys.argv[1]

edges = collections.defaultdict(set)   # (path,name) -> {(callee_path,callee_name)}
defs = {}                              # (path,name) -> span size
test_roots = set()

for line in open(f"{SCRATCH}/all_resolved.jsonl"):
    r = json.loads(line)
    if r["record"] == "resolved_edge":
        edges[(r["caller_path"], r["caller_name"])].add((r["callee_path"], r["callee_name"]))

for line in open(f"{SCRATCH}/defs.tsv"):
    f, js = line.rstrip("\n").split("\t", 1)
    r = json.loads(js)
    if r["kind"] in ("function", "method"):
        defs[(f, r["name"])] = r["span"]["end"] - r["span"]["start"]

roots, roots_test = set(), set()
for (path, name) in defs:
    if path.startswith("/Users/chrishafley/projects/typescript-go/cmd/") and name == "main":
        roots.add((path, name))
    if path.endswith("internal/compiler/program.go") and name[:1].isupper():
        roots.add((path, name))
    if path.endswith("_test.go"):
        roots_test.add((path, name))

def crawl(rootset):
    seen = {}
    frontier = [r for r in rootset if r in defs]
    for r in frontier:
        seen[r] = 0
    d = 0
    hist = collections.Counter()
    while frontier:
        hist[d] = len(frontier)
        nxt = []
        for node in frontier:
            for callee in edges.get(node, ()):
                if callee not in seen and callee in defs:
                    seen[callee] = d + 1
                    nxt.append(callee)
        frontier = nxt
        d += 1
    return seen, hist

seen, hist = crawl(roots)
seen_t, hist_t = crawl(roots | roots_test)

print(f"defs_total\t{len(defs)}")
print(f"roots\t{len(roots)}")
print(f"reachable\t{len(seen)}")
print(f"roots_with_tests\t{len(roots | roots_test)}")
print(f"reachable_with_tests\t{len(seen_t)}")
print("depth_hist\tdepth\tnewly_reachable\tcumulative")
cum = 0
for d in sorted(hist):
    cum += hist[d]
    print(f"depth\t{d}\t{hist[d]}\t{cum}")

unreach = [((p, n), sz) for (p, n), sz in defs.items() if (p, n) not in seen_t]
unreach.sort(key=lambda x: -x[1])
print("top_unreachable_by_span")
for (p, n), sz in unreach[:20]:
    print(f"unreachable\t{sz}\t{p}:{n}")

outdeg = sorted(((len(v), k) for k, v in edges.items() if k in defs), reverse=True)
print("top_outdegree")
for deg, (p, n) in outdeg[:20]:
    print(f"outdeg\t{deg}\t{p}:{n}")
