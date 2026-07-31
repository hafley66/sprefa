"""SLOT-SCALE: synthesize a layered file graph, run depth + cohesion + one cut
at growing sizes on both the referee and the reference engine, and record where
each one stops. The number that matters is the wall, not the shape of the curve.
"""
import json
import os
import random
import subprocess
import sys
import time
from pathlib import Path

import networkx as nx

LAB = Path(__file__).resolve().parent
OUT = LAB / "out"
REPO = LAB.parents[3]
SIZES = [int(s) for s in (sys.argv[1].split(",") if len(sys.argv) > 1 else
                          ["100", "300", "1000", "3000", "10000"])]


def synth(files, folders=50, fanout=3, layers=8, seed=7):
    """A layered DAG: every edge runs from a higher layer to a strictly lower
    one, which is the shape a real import graph has once cycles are removed."""
    rng = random.Random(seed)
    graph = nx.DiGraph()
    folder_of, layer_of = {}, {}
    for i in range(files):
        path = f"pkg/f{i % folders:03d}/file{i:06d}.ts"
        graph.add_node(path)
        folder_of[path] = f"pkg/f{i % folders:03d}"
        layer_of[path] = min(layers - 1, int(i / max(1, files / layers)))
    by_layer = {}
    for path, layer in layer_of.items():
        by_layer.setdefault(layer, []).append(path)
    for path, layer in layer_of.items():
        if layer == 0:
            continue
        for _ in range(fanout):
            target_layer = rng.randrange(0, layer)
            graph.add_edge(path, rng.choice(by_layer[target_layer]))
    return graph, folder_of


def heights(graph):
    condensed = nx.condensation(graph)
    height = {}
    for scc in reversed(list(nx.topological_sort(condensed))):
        successors = list(condensed.successors(scc))
        height[scc] = 0 if not successors else 1 + max(height[s] for s in successors)
    member = condensed.graph["mapping"]
    return {n: height[member[n]] for n in graph.nodes}


def referee_pass(graph, folder_of):
    marks = {}
    start = time.monotonic()
    depth = heights(graph)
    marks["depth_s"] = round(time.monotonic() - start, 3)

    start = time.monotonic()
    internal = crossing = 0
    for tail, head in graph.edges:
        if folder_of[tail] == folder_of[head]:
            internal += 1
        else:
            crossing += 1
    groups = {}
    for node, folder in folder_of.items():
        groups.setdefault(folder, set()).add(node)
    q = nx.community.modularity(graph.to_undirected(), list(groups.values()))
    marks["cohesion_s"] = round(time.monotonic() - start, 3)

    biggest = max(
        ((a, b) for a in groups for b in groups if a != b),
        key=lambda pair: sum(1 for t, h in graph.edges
                             if folder_of[t] == pair[0] and folder_of[h] == pair[1]))
    edges = [(t, h) for t, h in graph.edges
             if folder_of[t] == biggest[0] and folder_of[h] == biggest[1]]
    start = time.monotonic()
    after = graph.copy()
    after.remove_edges_from(edges)
    iface = "IFACE"
    for tail in {t for t, _ in edges}:
        after.add_edge(tail, iface)
    for head in {h for _, h in edges}:
        after.add_edge(iface, head)
    after_depth = max(heights(after).values())
    marks["cut_s"] = round(time.monotonic() - start, 3)

    marks.update(max_depth=max(depth.values()), internal=internal, crossing=crossing,
                 modularity=round(q, 4), cut_spliced=len(edges), cut_depth_after=after_depth,
                 transitive_pairs=None)
    return marks


def engine_pass(graph, folder_of, budget):
    batch = []
    for path, folder in folder_of.items():
        batch.append({"rel": "file_folder", "sign": "add", "row": [path, folder]})
    for tail, head in graph.edges:
        batch.append({"rel": "dep", "sign": "add", "row": [tail, head]})
    schedule = OUT / "scale.schedule.json"
    schedule.write_text(json.dumps([batch]))
    goal = f"oracle('{LAB / 'factorize.dl6'}','{schedule}')"
    start = time.monotonic()
    proc = subprocess.run(
        [str(LAB / "cap.sh"), str(budget), "scale oracle", "swipl", "-q", "-l",
         str(REPO / "v6/prolog/compile/scripts/dl6_oracle.pl"), "-g", goal, "-g", "halt"],
        capture_output=True, text=True, cwd=str(REPO / "v6/prolog/compile/scripts"))
    wall = round(time.monotonic() - start, 1)
    if proc.returncode == 124:
        return {"status": f"TIMEOUT at {budget}s", "wall_s": wall}
    if proc.returncode != 0:
        return {"status": "ERROR", "wall_s": wall, "stderr": proc.stderr.strip()[-300:]}
    lines = [l for l in proc.stdout.strip().split("\n") if l.startswith("{")]
    return {"status": "ok", "wall_s": wall, "ticklog_lines": len(lines),
            "ticklog_bytes": len(proc.stdout)}


def main():
    results = []
    for size in SIZES:
        graph, folder_of = synth(size)
        row = {"files": size, "edges": graph.number_of_edges()}
        row["referee"] = referee_pass(graph, folder_of)
        pairs = sum(len(nx.descendants(graph, n)) for n in graph.nodes) if size <= 3000 else None
        row["referee"]["transitive_pairs"] = pairs
        budget = 60 if size <= 300 else 300
        row["engine"] = ({"status": "skipped"} if os.environ.get("AF_REFEREE_ONLY")
                         else engine_pass(graph, folder_of, budget))
        results.append(row)
        print(json.dumps(row, sort_keys=True))
        if row["engine"]["status"] not in ("ok", "skipped"):
            print(f"engine wall reached at {size} files / {row['edges']} edges")
            break
    (OUT / ("scale-referee.json" if os.environ.get("AF_REFEREE_ONLY") else "scale.json")).write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
