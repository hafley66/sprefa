"""Grade cuts.dl6 against the referee: components against networkx
connected_components, and every folder-boundary cut's edge count and depth
against the same cut applied in networkx.
"""
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

import networkx as nx

LAB = Path(__file__).resolve().parent
OUT = LAB / "out"
REPO = LAB.parents[3]


def read_tsv(name):
    return [line.split("\t") for line in (OUT / name).read_text().strip().split("\n")]


def build():
    graph = nx.DiGraph()
    folder_of = {}
    for path, _package, folder, _origin in read_tsv("file_nodes.tsv"):
        graph.add_node(path)
        folder_of[path] = folder
    for tail, head, _plane in read_tsv("file_edges.tsv"):
        graph.add_edge(tail, head)
    return graph, folder_of


def candidates(graph, folder_of):
    buckets = defaultdict(list)
    for tail, head in graph.edges:
        a, b = folder_of[tail], folder_of[head]
        if a != b:
            buckets[f"{a}=>{b}"].append((tail, head))
    return dict(sorted(buckets.items()))


def write_schedules(graph, folder_of, cuts):
    component_batch = []
    for index, (path, folder) in enumerate(sorted(folder_of.items())):
        component_batch.append({"rel": "file_folder", "sign": "add", "row": [path, folder]})
        component_batch.append({"rel": "node_index", "sign": "add", "row": [path, index]})
    for tail, head in sorted(graph.edges):
        component_batch.append({"rel": "dep", "sign": "add", "row": [tail, head]})
    (OUT / "components.schedule.json").write_text(json.dumps([component_batch]) + "\n")

    cut_batch = [{"rel": "dep", "sign": "add", "row": [t, h]} for t, h in sorted(graph.edges)]
    for name, edges in cuts.items():
        cut_batch.append({"rel": "cut_iface", "sign": "add", "row": [name, f"IFACE:{name}"]})
        for tail, head in sorted(edges):
            cut_batch.append({"rel": "cut_edge", "sign": "add", "row": [name, tail, head]})
    (OUT / "cuts.schedule.json").write_text(json.dumps([cut_batch]) + "\n")
    return len(component_batch), len(cut_batch)


def run_oracle(program, schedule, log, budget):
    goal = f"oracle('{LAB / program}','{OUT / schedule}')"
    proc = subprocess.run(
        [str(LAB / "cap.sh"), str(budget), f"{program} oracle", "swipl", "-q", "-l",
         str(REPO / "v6/prolog/compile/scripts/dl6_oracle.pl"), "-g", goal, "-g", "halt"],
        capture_output=True, text=True, cwd=str(REPO / "v6/prolog/compile/scripts"))
    (OUT / log).write_text(proc.stdout)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr[-4000:])
    return proc.returncode


def final_state(log):
    rows = defaultdict(set)
    for line in (OUT / log).read_text().strip().split("\n"):
        if not line.startswith("{"):
            continue
        for rel, delta in json.loads(line)["deltas"].items():
            for row in delta.get("add", []):
                rows[rel].add(tuple(row))
            for row in delta.get("del", []):
                rows[rel].discard(tuple(row))
    return rows


def apply_cut(graph, name, edges):
    after = graph.copy()
    iface = f"IFACE:{name}"
    after.remove_edges_from(edges)
    after.add_node(iface)
    for tail in {t for t, _ in edges}:
        after.add_edge(tail, iface)
    for head in {h for _, h in edges}:
        after.add_edge(iface, head)
    return after


def heights(graph):
    condensed = nx.condensation(graph)
    height = {}
    for scc in reversed(list(nx.topological_sort(condensed))):
        successors = list(condensed.successors(scc))
        height[scc] = 0 if not successors else 1 + max(height[s] for s in successors)
    member = condensed.graph["mapping"]
    return {n: height[member[n]] for n in graph.nodes}


def main():
    graph, folder_of = build()
    cuts = candidates(graph, folder_of)
    component_arrivals, cut_arrivals = write_schedules(graph, folder_of, cuts)
    if run_oracle("components.dl6", "components.schedule.json", "components.ticklog.jsonl", 900) != 0:
        print("COMPONENTS ORACLE FAILED")
        return 1
    if run_oracle("cuts.dl6", "cuts.schedule.json", "cuts.ticklog.jsonl", 2400) != 0:
        print("CUTS ORACLE FAILED")
        return 1
    state = final_state("components.ticklog.jsonl")
    state.update(final_state("cuts.ticklog.jsonl"))

    undirected = graph.to_undirected()
    referee_components = {}
    for members in nx.connected_components(undirected):
        tag = min(members)
        for node in members:
            referee_components[node] = tag
    engine_components = {n: t for n, t in state.get("component", ())}
    component_mismatch = sorted(
        (n, referee_components.get(n), engine_components.get(n))
        for n in set(referee_components) | set(engine_components)
        if referee_components.get(n) != engine_components.get(n))

    engine_edges_after = {c: int(n) for c, n in state.get("edges_after", ())}
    engine_depth_after = {c: int(n) for c, n in state.get("max_depth_after", ())}
    engine_cyclic = {c for c, _node in state.get("cycle_after", ())}
    rows, edge_mismatch, depth_mismatch = [], [], []
    base_depth = max(heights(graph).values())
    for name, edges in cuts.items():
        after = apply_cut(graph, name, edges)
        referee_edges = after.number_of_edges()
        referee_depth = max(heights(after).values())
        rows.append({
            "cut": name, "spliced": len(edges),
            "edges_before": graph.number_of_edges(), "edges_after": referee_edges,
            "edge_delta": referee_edges - graph.number_of_edges(),
            "depth_before": base_depth, "depth_after": referee_depth,
        })
        if engine_edges_after.get(name) != referee_edges:
            edge_mismatch.append((name, referee_edges, engine_edges_after.get(name)))
        # A cut that closes a cycle has no bounded-recursion depth, so the claim
        # graded there is that the engine SAYS SO, not that the number matches.
        cyclic_referee = any(len(c) > 1 for c in nx.strongly_connected_components(after))
        engine_flagged = name in engine_cyclic
        if cyclic_referee != engine_flagged:
            depth_mismatch.append(("cycle flag", name, cyclic_referee, engine_flagged))
        elif not cyclic_referee and engine_depth_after.get(name) != referee_depth:
            depth_mismatch.append((name, referee_depth, engine_depth_after.get(name)))
        rows[-1]["cyclic_after_cut"] = cyclic_referee

    rows.sort(key=lambda r: (r["edge_delta"], -r["spliced"]))
    verdict = {
        "component_arrivals": component_arrivals,
        "cut_arrivals": cut_arrivals,
        "cuts": len(cuts),
        "component_count_referee": len(set(referee_components.values())),
        "component_mismatch": component_mismatch,
        "engine_component_sizes": sorted(
            (int(n), t) for t, n in state.get("component_size", ())),
        "edge_mismatch": edge_mismatch,
        "depth_mismatch": depth_mismatch,
        "engine_cyclic_cuts": sorted(engine_cyclic),
        "ranked": rows,
    }
    (OUT / "cuts_grade.json").write_text(json.dumps(verdict, indent=2, sort_keys=True) + "\n")
    print(json.dumps(verdict, indent=2, sort_keys=True))
    holds = not component_mismatch and not edge_mismatch and not depth_mismatch
    print("CUTS GRADE HOLDS" if holds else "CUTS GRADE FAILS")
    return 0 if holds else 1


if __name__ == "__main__":
    sys.exit(main())
