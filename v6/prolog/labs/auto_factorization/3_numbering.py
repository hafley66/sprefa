"""Q6 referee: derive the numeric file prefix from dependency depth.

The prefix is a DENSE rank, so files at one depth share a number; the tiebreak
orders the listing inside a layer and never splits it. 8_classify.py owns the
disagreement buckets.
"""
import json
import re
from collections import defaultdict
from pathlib import Path

import networkx as nx

LAB = Path(__file__).resolve().parent
OUT = LAB / "out"
PREFIX = re.compile(r"^(\d+)_")


def read_tsv(name):
    return [line.split("\t") for line in (OUT / name).read_text().strip().split("\n")]


def load():
    graph = nx.DiGraph()
    meta = {}
    for path, package, folder, origin in read_tsv("file_nodes.tsv"):
        graph.add_node(path)
        meta[path] = {"package": package, "folder": folder, "origin": origin}
    for tail, head, _plane in read_tsv("file_edges.tsv"):
        graph.add_edge(tail, head)
    return graph, meta


def heights(graph):
    condensed = nx.condensation(graph)
    height = {}
    for scc in reversed(list(nx.topological_sort(condensed))):
        successors = list(condensed.successors(scc))
        height[scc] = 0 if not successors else 1 + max(height[s] for s in successors)
    member = condensed.graph["mapping"]
    return {n: height[member[n]] for n in graph.nodes}


def hand_prefix(path):
    match = PREFIX.match(path.rsplit("/", 1)[1])
    return int(match.group(1)) if match else None


def main():
    graph, meta = load()
    packages = sorted({m["package"] for m in meta.values()})
    report = {"packages": {}, "totals": defaultdict(int)}
    table_rows = []

    for package in packages:
        members = [n for n in graph.nodes if meta[n]["package"] == package]
        sub = graph.subgraph(members).copy()
        depth = heights(sub)

        # Folder ordering: quotient the package graph by folder, then take the
        # same height. A folder depending on another sits above it.
        quotient = nx.DiGraph()
        quotient.add_nodes_from({meta[n]["folder"] for n in members})
        for tail, head in sub.edges:
            a, b = meta[tail]["folder"], meta[head]["folder"]
            if a != b:
                quotient.add_edge(a, b)
        folder_depth = heights(quotient)
        folder_rank = {f: r for r, f in enumerate(
            sorted(quotient.nodes, key=lambda f: (folder_depth[f], f)))}

        # Prefix = dense rank of depth WITHIN the folder. Files sharing a depth
        # share a prefix, which is what the existing convention already does.
        per_folder = defaultdict(list)
        for node in members:
            per_folder[meta[node]["folder"]].append(node)
        derived = {}
        for folder, nodes in per_folder.items():
            ladder = sorted({depth[n] for n in nodes})
            rank_of = {d: i for i, d in enumerate(ladder)}
            for node in nodes:
                derived[node] = rank_of[depth[node]]

        # Tiebreak inside one prefix: most depended-upon first, then name.
        order_key = {n: (derived[n], -sub.in_degree(n), n) for n in members}

        uphill = []
        for tail, head in sub.edges:
            a, b = hand_prefix(tail), hand_prefix(head)
            if a is not None and b is not None and b > a:
                uphill.append({"importer": tail, "imports": head,
                               "hand_importer": a, "hand_imported": b})

        rows = []
        for node in sorted(members, key=lambda n: order_key[n]):
            hand = hand_prefix(node)
            folder = meta[node]["folder"]
            base = node.rsplit("/", 1)[1]
            stem = PREFIX.sub("", base)
            proposed = f"{folder}/{derived[node]}_{stem}"
            rows.append({
                "path": node, "folder": folder, "folder_rank": folder_rank[folder],
                "depth": depth[node], "hand": hand, "derived": derived[node],
                "in_degree": sub.in_degree(node), "out_degree": sub.out_degree(node),
                "origin": meta[node]["origin"],
                "proposed": proposed, "changes": proposed != node,
            })
        table_rows.extend(rows)

        numbered = [r for r in rows if r["hand"] is not None]
        agree = [r for r in numbered if r["hand"] == r["derived"]]
        report["packages"][package] = {
            "files": len(members), "edges": sub.number_of_edges(),
            "max_depth": max(depth.values()) if depth else 0,
            "folders": {f: {"rank": folder_rank[f], "depth": folder_depth[f]} for f in sorted(quotient.nodes)},
            "hand_numbered": len(numbered), "unnumbered": len(members) - len(numbered),
            "number_agrees": len(agree), "number_differs": len(numbered) - len(agree),
            "uphill_edges": uphill,
        }
        report["totals"]["files"] += len(members)
        report["totals"]["numbered"] += len(numbered)
        report["totals"]["agree"] += len(agree)
        report["totals"]["uphill"] += len(uphill)

    (OUT / "rename_table.json").write_text(json.dumps(
        {"report": report, "rows": table_rows}, indent=2, sort_keys=True, default=str) + "\n")

    header = ["package", "current_path", "depth", "hand", "derived", "in", "out", "proposed_path", "verdict"]
    lines = ["\t".join(header)]
    for row in table_rows:
        verdict = ("generated" if row["origin"] == "generated"
                   else "unnumbered" if row["hand"] is None
                   else "agree" if row["hand"] == row["derived"] else "differs")
        lines.append("\t".join(str(x) for x in [
            meta[row["path"]]["package"], row["path"], row["depth"], row["hand"],
            row["derived"], row["in_degree"], row["out_degree"], row["proposed"], verdict]))
    (OUT / "rename_table.tsv").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    print()
    print(json.dumps(report, indent=2, sort_keys=True, default=str))


if __name__ == "__main__":
    main()
