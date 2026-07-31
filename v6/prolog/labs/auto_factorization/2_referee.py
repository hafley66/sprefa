"""Offline referee. Every number the dl6 side claims is checked against this.

networkx supplies toposort, condensation, modularity and louvain_communities;
nothing graph-theoretic here is hand-rolled except the counting loops the
answer key needs printed row by row.
"""
import json
from collections import defaultdict
from pathlib import Path

import networkx as nx

LAB = Path(__file__).resolve().parent
OUT = LAB / "out"

# Package = the rename unit. gen_emitted / gen_served are compiler OUTPUT and
# carry no hand prefix to disagree with, so they are outside every axis here.
PACKAGES = {
    "tsv2": ("v6/tsv2/cli/", "v6/tsv2/serve/", "v6/tsv2/runtime/"),
    "dl": ("v6/dl/src/",),
    "prolog": ("v6/prolog/",),
}
GENERATED = ("v6/dl/src/0_generated/",)


def read_tsv(name):
    return [line.split("\t") for line in (OUT / name).read_text().strip().split("\n")]


def package_of(path):
    for name, prefixes in PACKAGES.items():
        if any(path.startswith(p) for p in prefixes):
            return name
    return None


def file_graph():
    """One directed graph over FILES: an edge means the tail depends on the head."""
    graph = nx.DiGraph()
    node_path, node_plane = {}, {}
    for node_id, plane, path, _symbol in read_tsv("nodes.tsv"):
        node_path[node_id] = path
        node_plane[node_id] = plane

    def add(tail, head, plane):
        if package_of(tail) is None or package_of(head) is None:
            return
        graph.add_node(tail, plane=plane)
        graph.add_node(head, plane=plane)
        if tail != head:
            graph.add_edge(tail, head, plane=plane)

    for tail, head in read_tsv("ts_file_imports.tsv"):
        add(tail, head, "typescript")
    for tail, head, kind in read_tsv("edges.tsv"):
        if kind != "calls" or node_plane[tail] != "prolog" or node_plane[head] != "prolog":
            continue
        add(node_path[tail], node_path[head], "prolog")
    for path, plane in ((p, pl) for p, pl in zip(node_path.values(), node_plane.values())):
        if plane == "prolog" and package_of(path):
            graph.add_node(path, plane="prolog")
    return graph


def depth_map(graph):
    """Height above the dependency leaves, over the SCC condensation.

    A file depending on nothing inside the analysed set is 0. Every cycle
    collapses to one condensation node, so all its members share one depth;
    that is what makes the number defined at all when imports cycle.
    """
    condensed = nx.condensation(graph)
    height = {}
    for scc in reversed(list(nx.topological_sort(condensed))):
        successors = list(condensed.successors(scc))
        height[scc] = 0 if not successors else 1 + max(height[s] for s in successors)
    member_of = condensed.graph["mapping"]
    return {node: height[member_of[node]] for node in graph.nodes}, condensed, member_of


def internal_cross(graph, group_of):
    internal, cross = defaultdict(int), defaultdict(int)
    for tail, head in graph.edges:
        a, b = group_of.get(tail), group_of.get(head)
        if a is None or b is None:
            continue
        if a == b:
            internal[a] += 1
        else:
            cross[a] += 1
            cross[b] += 1
    return internal, cross


def modularity(graph, group_of):
    groups = defaultdict(set)
    covered = set()
    for node, g in group_of.items():
        if node in graph:
            groups[g].add(node)
            covered.add(node)
    leftover = set(graph.nodes) - covered
    parts = list(groups.values()) + [{n} for n in leftover]
    if not parts:
        return 0.0
    return nx.community.modularity(graph.to_undirected(), parts)


def folder_of(path):
    return path.rsplit("/", 1)[0]


def emit(rows, name):
    (OUT / name).write_text("\n".join("\t".join(str(c) for c in r) for r in rows) + "\n")


def axes_for(graph):
    return {
        "file": {n: n for n in graph.nodes},
        "folder": {n: folder_of(n) for n in graph.nodes},
        "package": {n: package_of(n) for n in graph.nodes},
        "plane": {n: graph.nodes[n].get("plane", "typescript") for n in graph.nodes},
    }


def cohesion_report(graph):
    out = {}
    for axis, group_of in axes_for(graph).items():
        internal, cross = internal_cross(graph, group_of)
        groups = sorted(set(group_of.values()))
        out[axis] = {
            "groups": len(groups),
            "internal": sum(internal.values()),
            "cross": sum(cross.values()) // 2,
            "modularity": round(modularity(graph, group_of), 4),
            "per_group": {
                str(g): {
                    "internal": internal[g],
                    "cross": cross[g],
                    "ratio": round(internal[g] / (internal[g] + cross[g]), 4) if internal[g] + cross[g] else None,
                }
                for g in groups
            },
        }
    return out


def main():
    graph = file_graph()
    report = {}

    cycles = [sorted(c) for c in nx.strongly_connected_components(graph) if len(c) > 1]
    report["file_graph"] = {
        "files": graph.number_of_nodes(),
        "edges": graph.number_of_edges(),
        "cycles": len(cycles),
        "cycle_members": sorted(cycles),
        "per_package": {p: sum(1 for n in graph.nodes if package_of(n) == p) for p in PACKAGES},
    }

    depths, condensed, _member = depth_map(graph)
    emit(sorted((p, d) for p, d in depths.items()), "depth.tsv")
    report["depth"] = {
        "max": max(depths.values()),
        "condensation_nodes": condensed.number_of_nodes(),
        "histogram": {str(d): sum(1 for v in depths.values() if v == d) for d in sorted(set(depths.values()))},
    }

    cohesion = cohesion_report(graph)
    report["cohesion"] = cohesion

    undirected = graph.to_undirected()
    louvain = nx.community.louvain_communities(undirected, seed=1)
    lpa = list(nx.community.asyn_lpa_communities(undirected, seed=1))
    greedy = list(nx.community.greedy_modularity_communities(undirected))
    report["community"] = {
        "louvain": {"count": len(louvain), "modularity": round(nx.community.modularity(undirected, louvain), 4),
                    "sizes": sorted((len(c) for c in louvain), reverse=True)},
        "label_propagation": {"count": len(lpa), "modularity": round(nx.community.modularity(undirected, lpa), 4),
                              "sizes": sorted((len(c) for c in lpa), reverse=True)},
        "greedy_modularity": {"count": len(greedy), "modularity": round(nx.community.modularity(undirected, greedy), 4),
                              "sizes": sorted((len(c) for c in greedy), reverse=True)},
        "folder_partition_modularity": cohesion["folder"]["modularity"],
        "package_partition_modularity": cohesion["package"]["modularity"],
    }
    louvain_of = {}
    for index, community in enumerate(sorted(louvain, key=lambda c: sorted(c)[0])):
        for node in community:
            louvain_of[node] = index
    emit(sorted((n, louvain_of[n]) for n in louvain_of), "louvain.tsv")

    edge_rows = sorted((t, h, graph.edges[t, h]["plane"]) for t, h in graph.edges)
    emit(edge_rows, "file_edges.tsv")
    emit(sorted((n, package_of(n), folder_of(n),
                 "generated" if n.startswith(GENERATED) else "source") for n in graph.nodes), "file_nodes.tsv")

    (OUT / "referee.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k != "cohesion"}, indent=2, sort_keys=True))
    for axis, data in cohesion.items():
        print(f"cohesion\t{axis}\tgroups={data['groups']}\tinternal={data['internal']}"
              f"\tcross={data['cross']}\tQ={data['modularity']}")


if __name__ == "__main__":
    main()
