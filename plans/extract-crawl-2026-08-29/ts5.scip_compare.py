#!/usr/bin/env python3
"""Crawl scip_fn_edge from the same entrypoints, then diff against resolved_edge.

Both graphs are keyed on (repo-relative file, symbol name); scip_fn_edge carries
no span, so same-name defs in one file fold together on both sides alike.
"""
import collections, json, random, re

S = "/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/crawl-ts5"
P = "/Users/chrishafley/projects/TypeScript-5.9/"
FN = re.compile(r"\)\.$")

sym_file, sym_name, fn_edges = {}, {}, []
for l in open(S + "/scip.jsonl"):
    o = json.loads(l)
    r = o["record"]
    if r == "scip_def":
        sym_file[o["symbol"]] = o["file"]
    elif r == "scip_name":
        sym_name[o["symbol"]] = o["name"]
    elif r == "scip_fn_edge":
        fn_edges.append((o["caller"], o["callee"]))


def key(sym):
    f, n = sym_file.get(sym), sym_name.get(sym)
    return (f, n) if f and n else None


scip_out = collections.defaultdict(set)
scip_edge_set = set()
for a, b in fn_edges:
    ka, kb = key(a), key(b)
    if ka and kb:
        scip_out[ka].add(kb)
        scip_edge_set.add((ka, kb))
scip_defs = {key(s) for s in sym_file if FN.search(s)} - {None}

diet_nodes = set()
for l in open(S + "/src.call.jsonl"):
    o = json.loads(l)
    if o["record"] == "node" and o["kind"] in ("function", "method") and o["name"]:
        diet_nodes.add((o["path"], o["name"]))
diet_out = collections.defaultdict(set)
diet_edge_set = set()
for l in open(S + "/src.resolve.jsonl"):
    o = json.loads(l)
    if o["record"] != "resolved_edge":
        continue
    a = (o["caller_path"].replace(P, ""), o["caller_name"])
    b = (o["callee_path"].replace(P, ""), o["callee_name"])
    diet_out[a].add(b)
    diet_edge_set.add((a, b))

scip_files = set(sym_file.values())
my_files = set(l.strip() for l in open(S + "/files_src.txt"))
COMMON = (scip_files & my_files)
MISSING = my_files - scip_files
shared_defs = {k for k in (scip_defs & diet_nodes) if k[0] in COMMON}

print("files: mine %d, scip src docs %d, both %d, in mine only %s"
      % (len(my_files), len([f for f in scip_files if f.startswith("src/")]),
         len(COMMON), sorted(MISSING)))
print("defs: diet %d, scip fn-descriptor %d, shared (both, in COMMON) %d"
      % (len(diet_nodes), len(scip_defs), len(shared_defs)))
print("edges: diet %d, scip_fn_edge %d keyed %d" % (len(diet_edge_set), len(fn_edges), len(scip_edge_set)))

ENTRY = ["src/tsc/tsc.ts", "src/tsserver/server.ts", "src/typescript/typescript.ts"]


def exported_functions(path):
    names = set()
    for ln in open(P + path, encoding="utf8", errors="replace"):
        if ln.startswith("export function "):
            names.add(ln[len("export function "):].split("(")[0].split("<")[0].strip())
    return {(path, n) for n in names}


def bfs(seed, graph):
    d = {k: 0 for k in seed}
    q = collections.deque(d)
    while q:
        u = q.popleft()
        for v in graph.get(u, ()):
            if v not in d:
                d[v] = d[u] + 1
                q.append(v)
    return d


rows = []
for label, defs, graph in (("diet", diet_nodes, diet_out), ("scip", scip_defs, scip_out)):
    sa = {k for k in defs if k[0] in ENTRY} | (exported_functions("src/compiler/program.ts") & defs)
    sb = {k for k in defs if k[0].startswith("src/testRunner/")}
    da, db = bfs(sa, graph), bfs(sb, graph)
    uni = set(da) | set(db)
    rows.append((label, len(sa), len(sb), len(set(da) & shared_defs), len(set(db) & shared_defs),
                 len(uni & shared_defs), max(da.values()) if da else 0))
print("\nreachability over the %d shared defs" % len(shared_defs))
print(f"{'side':5s} {'seedA':>6s} {'seedB':>6s} {'A':>6s} {'B':>6s} {'union':>6s} {'pct':>6s} {'maxdA':>6s}")
for lb, sa, sb, a, b, u, md in rows:
    print(f"{lb:5s} {sa:6d} {sb:6d} {a:6d} {b:6d} {u:6d} {u/len(shared_defs)*100:5.1f}% {md:6d}")


def restrict(es):
    return {(a, b) for a, b in es if a in shared_defs and b in shared_defs}


ds, ss = restrict(diet_edge_set), restrict(scip_edge_set)
print("\nedges restricted to shared defs on both ends:")
print("  diet %d  scip %d  shared %d  diet-only %d  scip-only %d"
      % (len(ds), len(ss), len(ds & ss), len(ds - ss), len(ss - ds)))

random.seed(11)
with open(S + "/ts5.scip_samples.tsv", "w") as fh:
    fh.write("side\tcaller_file\tcaller_name\tcallee_file\tcallee_name\n")
    for side, s in (("diet_only", ds - ss), ("scip_only", ss - ds)):
        for a, b in random.sample(sorted(s), 15):
            fh.write(f"{side}\t{a[0]}\t{a[1]}\t{b[0]}\t{b[1]}\n")
print("wrote ts5.scip_samples.tsv")
