"""The other cut granularity: relocate ONE file to another folder inside its own
package and re-measure. Exhaustive over every (file, folder) pair, no search
heuristic, smallest correct first.

The ranking key is folder-quotient cycles removed, then modularity gain, then
crossing-edge reduction. A folder pair with edges both ways has no layering at
all, which is a stronger defect than any ratio.
"""
import json
from collections import defaultdict
from pathlib import Path

import networkx as nx

LAB = Path(__file__).resolve().parent
OUT = LAB / "out"


def read_tsv(name):
    return [line.split("\t") for line in (OUT / name).read_text().strip().split("\n")]


def quotient_cycles(graph, folder_of):
    quotient = nx.DiGraph()
    quotient.add_nodes_from(set(folder_of.values()))
    for tail, head in graph.edges:
        a, b = folder_of[tail], folder_of[head]
        if a != b:
            quotient.add_edge(a, b)
    return sum(1 for c in nx.strongly_connected_components(quotient) if len(c) > 1), quotient


def measure(graph, folder_of):
    cycles, quotient = quotient_cycles(graph, folder_of)
    internal = crossing = 0
    for tail, head in graph.edges:
        if folder_of[tail] == folder_of[head]:
            internal += 1
        else:
            crossing += 1
    groups = defaultdict(set)
    for node, folder in folder_of.items():
        groups[folder].add(node)
    q = nx.community.modularity(graph.to_undirected(), list(groups.values()))
    return {"folder_cycles": cycles, "internal": internal, "crossing": crossing,
            "modularity": round(q, 4), "quotient_edges": quotient.number_of_edges()}


def minimal_relabel(graph, folder_of, low, high):
    """Fewest file moves making every edge run high -> low, i.e. deleting the
    folder cycle between exactly two folders.

    This is the minimum-cost closure problem: choose x(f) in {low, high} with
    x(tail) >= x(head) on every edge, minimising disagreement with the current
    labels. Solved exactly as a min cut on the standard construction, not by a
    heuristic and not by enumerating subsets.
    """
    members = [n for n in graph.nodes if folder_of[n] in (low, high)]
    inside = set(members)
    flow = nx.DiGraph()
    source, sink = "<SRC>", "<SNK>"
    infinite = len(members) + 1
    for node in members:
        if folder_of[node] == high:
            flow.add_edge(source, node, capacity=1)
        else:
            flow.add_edge(node, sink, capacity=1)
    for tail, head in graph.edges:
        if tail in inside and head in inside:
            flow.add_edge(head, tail, capacity=infinite)
    cut_value, (side_high, _side_low) = nx.minimum_cut(flow, source, sink)
    assignment = {n: (high if n in side_high else low) for n in members}
    moves = sorted(n for n in members if assignment[n] != folder_of[n])
    return cut_value, moves, assignment


def main():
    graph = nx.DiGraph()
    folder_of, package_of = {}, {}
    for path, package, folder, _origin in read_tsv("file_nodes.tsv"):
        graph.add_node(path)
        folder_of[path] = folder
        package_of[path] = package
    for tail, head, _plane in read_tsv("file_edges.tsv"):
        graph.add_edge(tail, head)

    base = measure(graph, folder_of)
    folders_in = defaultdict(set)
    for path, folder in folder_of.items():
        folders_in[package_of[path]].add(folder)

    rows = []
    for path in sorted(graph.nodes):
        for target in sorted(folders_in[package_of[path]]):
            if target == folder_of[path]:
                continue
            moved = dict(folder_of)
            moved[path] = target
            after = measure(graph, moved)
            rows.append({
                "file": path, "from": folder_of[path], "to": target,
                "folder_cycles": [base["folder_cycles"], after["folder_cycles"]],
                "crossing": [base["crossing"], after["crossing"]],
                "modularity": [base["modularity"], after["modularity"]],
                "cycle_delta": after["folder_cycles"] - base["folder_cycles"],
                "crossing_delta": after["crossing"] - base["crossing"],
                "modularity_delta": round(after["modularity"] - base["modularity"], 4),
            })
    rows.sort(key=lambda r: (r["cycle_delta"], -r["modularity_delta"], r["crossing_delta"]))

    cycles, quotient = quotient_cycles(graph, folder_of)
    minimal = []
    for component in nx.strongly_connected_components(quotient):
        if len(component) != 2:
            continue
        first, second = sorted(component)
        for low, high in ((first, second), (second, first)):
            size, moves, assignment = minimal_relabel(graph, folder_of, low, high)
            after = measure(graph, {**folder_of, **assignment})
            minimal.append({"low": low, "high": high, "moves": len(moves), "files": moves,
                            "folder_cycles_after": after["folder_cycles"],
                            "crossing_after": after["crossing"],
                            "modularity_after": after["modularity"],
                            "min_cut_value": size})
    minimal.sort(key=lambda r: r["moves"])

    (OUT / "moves.json").write_text(json.dumps(
        {"base": base, "rows": rows, "minimal_relabelings": minimal}, indent=2, sort_keys=True) + "\n")
    print("\n== minimal file moves that delete a folder cycle (exact, min cut) ==")
    for row in minimal:
        print(f"  keep {row['high']} above {row['low']}: {row['moves']} moves, "
              f"cycles -> {row['folder_cycles_after']}, crossing {base['crossing']} -> "
              f"{row['crossing_after']}, Q {base['modularity']} -> {row['modularity_after']}")
        for path in row["files"]:
            print(f"      {path}")

    print(json.dumps(base, sort_keys=True))
    print("\nfile\tfrom\tto\tcycles\tcrossing\tQ\tdQ")
    for row in rows[:12]:
        print("\t".join(str(x) for x in [
            row["file"], row["from"], row["to"],
            f"{row['folder_cycles'][0]}->{row['folder_cycles'][1]}",
            f"{row['crossing'][0]}->{row['crossing'][1]}",
            f"{row['modularity'][0]}->{row['modularity'][1]}",
            row["modularity_delta"]]))


if __name__ == "__main__":
    main()
