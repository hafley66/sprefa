"""Q4 + Q5: a proposed interface cut is a HYPOTHETICAL node splicing a set of
edges, and the search over candidate cuts is exhaustive over boundaries.

cut(name, edge_set) applied to a graph:
  every edge in edge_set is deleted; one node IFACE:<name> is added; every
  distinct tail gains tail -> IFACE and every distinct head gains IFACE -> head.
  N*M edges become N+M.

Two things this makes visibly awkward, both stated rather than smoothed over:
  the node set CHANGES, so modularity before and after are computed over
  different graphs and are not the same quantity; and the new node belongs to
  no existing group, so Q is reported with it both isolated and folded into the
  tail side.
"""
import json
from collections import defaultdict
from itertools import combinations
from pathlib import Path

import networkx as nx

LAB = Path(__file__).resolve().parent
OUT = LAB / "out"


def read_tsv(name):
    return [line.split("\t") for line in (OUT / name).read_text().strip().split("\n")]


def heights(graph):
    condensed = nx.condensation(graph)
    height = {}
    for scc in reversed(list(nx.topological_sort(condensed))):
        successors = list(condensed.successors(scc))
        height[scc] = 0 if not successors else 1 + max(height[s] for s in successors)
    member = condensed.graph["mapping"]
    return {n: height[member[n]] for n in graph.nodes}


def apply_cut(graph, name, edge_set):
    after = graph.copy()
    iface = f"IFACE:{name}"
    tails = sorted({t for t, _ in edge_set})
    heads = sorted({h for _, h in edge_set})
    after.remove_edges_from(edge_set)
    after.add_node(iface)
    for tail in tails:
        after.add_edge(tail, iface)
    for head in heads:
        after.add_edge(iface, head)
    return after, iface, tails, heads


def modularity(graph, group_of, extra=None):
    groups = defaultdict(set)
    for node in graph.nodes:
        groups[(extra or {}).get(node, group_of.get(node, node))].add(node)
    return nx.community.modularity(graph.to_undirected(), list(groups.values()))


def max_cluster(graph):
    undirected = graph.to_undirected()
    communities = nx.community.louvain_communities(undirected, seed=1)
    return max(len(c) for c in communities), len(communities), round(
        nx.community.modularity(undirected, communities), 4)


def score(graph, group_of, name, edge_set):
    after, iface, tails, heads = apply_cut(graph, name, edge_set)
    before_depth = max(heights(graph).values())
    after_depth = max(heights(after).values())
    before_max, before_k, before_louvain = max_cluster(graph)
    after_max, after_k, after_louvain = max_cluster(after)
    tail_group = group_of.get(tails[0])
    return {
        "cut": name,
        "spliced_edges": len(edge_set),
        "tails": len(tails),
        "heads": len(heads),
        "edges_before": graph.number_of_edges(),
        "edges_after": after.number_of_edges(),
        "edge_delta": after.number_of_edges() - graph.number_of_edges(),
        "depth_before": before_depth,
        "depth_after": after_depth,
        "louvain_max_cluster_before": before_max,
        "louvain_max_cluster_after": after_max,
        "louvain_k_before": before_k,
        "louvain_k_after": after_k,
        "louvain_q_before": before_louvain,
        "louvain_q_after": after_louvain,
        "q_reference_before": round(modularity(graph, group_of), 4),
        "q_reference_after_iface_isolated": round(modularity(after, group_of, {iface: iface}), 4),
        "q_reference_after_iface_in_tail_group": round(modularity(after, group_of, {iface: tail_group}), 4),
    }


def file_level():
    graph = nx.DiGraph()
    folder_of = {}
    for path, _package, folder, _origin in read_tsv("file_nodes.tsv"):
        graph.add_node(path)
        folder_of[path] = folder
    for tail, head, _plane in read_tsv("file_edges.tsv"):
        graph.add_edge(tail, head)
    return graph, folder_of


def symbol_level():
    graph = nx.DiGraph()
    group_of = {}
    for node_id, plane, path, _symbol in read_tsv("nodes.tsv"):
        graph.add_node(node_id)
        group_of[node_id] = path if path.startswith("v6/") else f"<{plane}>"
    kinds = {}
    for tail, head, kind in read_tsv("edges.tsv"):
        if kind == "resides":
            continue
        graph.add_edge(tail, head)
        kinds.setdefault(kind, []).append((tail, head))
    return graph, group_of, kinds


def boundary_candidates(graph, group_of):
    """Exhaustive over ORDERED group pairs carrying at least one edge."""
    buckets = defaultdict(list)
    for tail, head in graph.edges:
        a, b = group_of.get(tail), group_of.get(head)
        if a is not None and b is not None and a != b:
            buckets[(a, b)].append((tail, head))
    return buckets


def main():
    result = {}

    graph, folder_of = file_level()
    buckets = boundary_candidates(graph, folder_of)
    rows = [score(graph, folder_of, f"{a}=>{b}", edges) for (a, b), edges in sorted(buckets.items())]
    rows.sort(key=lambda r: (r["edge_delta"], -r["spliced_edges"]))
    result["file_folder_boundaries"] = rows

    # Folder-quotient cycles: a folder pair with edges in BOTH directions has no
    # layering at all, which is the strongest cut motive the boundary set holds.
    both_ways = sorted({tuple(sorted(pair)) for pair in buckets
                        if (pair[1], pair[0]) in buckets})
    result["folder_cycles"] = [
        {"folders": list(pair),
         "forward": len(buckets[(pair[0], pair[1])]),
         "backward": len(buckets[(pair[1], pair[0])])}
        for pair in both_ways]

    sym, sym_group, kinds = symbol_level()
    sym_buckets = boundary_candidates(sym, sym_group)
    sym_rows = [score(sym, sym_group, f"{a}=>{b}", edges)
                for (a, b), edges in sorted(sym_buckets.items()) if len(edges) >= 3]
    sym_rows.sort(key=lambda r: (r["edge_delta"], -r["spliced_edges"]))
    result["symbol_file_boundaries_top"] = sym_rows[:15]
    result["symbol_boundary_count"] = len(sym_buckets)

    bridges = [e for kind, edges in kinds.items() if kind == "bridge" for e in edges]
    result["bridge_family_cut"] = score(sym, sym_group, "all_bridges", bridges)
    per_kind = {}
    for kind, edges in sorted(kinds.items()):
        if kind == "bridge" or len(edges) < 3:
            continue
        per_kind[kind] = score(sym, sym_group, f"kind_{kind}", edges)
    result["symbol_kind_cuts"] = per_kind

    (OUT / "cuts.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")

    print("== folder cycles (no layering exists across these pairs) ==")
    for row in result["folder_cycles"]:
        print(f"  {row['folders'][0]} <-> {row['folders'][1]}  {row['forward']}/{row['backward']}")
    print()
    header = ["cut", "spliced", "tails", "heads", "edge_delta", "depth", "maxclus", "Q_ref_iso"]
    print("\t".join(header))
    for row in rows:
        print("\t".join(str(x) for x in [
            row["cut"], row["spliced_edges"], row["tails"], row["heads"], row["edge_delta"],
            f"{row['depth_before']}->{row['depth_after']}",
            f"{row['louvain_max_cluster_before']}->{row['louvain_max_cluster_after']}",
            f"{row['q_reference_before']}->{row['q_reference_after_iface_isolated']}"]))
    print()
    print("== symbol level, top boundaries by edge reduction ==")
    for row in sym_rows[:10]:
        print("\t".join(str(x) for x in [
            row["cut"], row["spliced_edges"], row["tails"], row["heads"], row["edge_delta"],
            f"{row['depth_before']}->{row['depth_after']}",
            f"{row['louvain_max_cluster_before']}->{row['louvain_max_cluster_after']}"]))
    print()
    print("== bridge family cut ==")
    print(json.dumps(result["bridge_family_cut"], indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
