"""Classify every disagreement between the derived prefix and the hand one.

Three buckets, and which bucket a row lands in is decided by the dependency
closure rather than by taste:

  HAND ERROR      some dependency path A -> B has hand(A) < hand(B). The hand
                  numbering contradicts itself; nothing about the metric is
                  needed to see it.
  SCALE SHIFT     numbers differ, every dependency-implied order still holds
                  under the hand numbering. The hand ladder is looser or offset,
                  which is a presentation difference and not an error either way.
  METRIC BLIND    derived depth is 0 with zero in-package dependencies while the
                  hand number is high. Candidate for a dependency the fact base
                  cannot see; the evidence column carries the out-of-package
                  import count so it can be adjudicated rather than asserted.
"""
import json
import re
from collections import defaultdict
from pathlib import Path

import networkx as nx

LAB = Path(__file__).resolve().parent
OUT = LAB / "out"
REPO = LAB.parents[3]
PREFIX = re.compile(r"^(\d+)_")


def read_tsv(name):
    return [line.split("\t") for line in (OUT / name).read_text().strip().split("\n")]


def hand_prefix(path):
    match = PREFIX.match(path.rsplit("/", 1)[1])
    return int(match.group(1)) if match else None


def external_deps(path):
    """Specifiers a TypeScript file imports from OUTSIDE the analysed set."""
    full = REPO / path
    if not full.exists() or not path.endswith(".ts"):
        return None
    text = full.read_text()
    return len({m.group(1) for m in re.finditer(r'(?:from|import)\s*\(?\s*["\']([^"\']+)["\']', text)
                if not m.group(1).startswith(".")})


def main():
    rename = json.loads((OUT / "rename_table.json").read_text())
    rows = {r["path"]: r for r in rename["rows"]}
    meta = {p: {"package": pk, "folder": fo, "origin": og} for p, pk, fo, og in read_tsv("file_nodes.tsv")}

    graph = nx.DiGraph()
    graph.add_nodes_from(meta)
    for tail, head, _plane in read_tsv("file_edges.tsv"):
        graph.add_edge(tail, head)
    closure = {n: nx.descendants(graph, n) for n in graph.nodes}

    hand_violations, derived_violations = [], []
    for tail in graph.nodes:
        for head in closure[tail]:
            if meta[tail]["folder"] != meta[head]["folder"]:
                continue
            a, b = hand_prefix(tail), hand_prefix(head)
            if a is not None and b is not None and a < b:
                hand_violations.append({"depends": tail, "on": head, "hand": [a, b]})
            if rows[tail]["derived"] < rows[head]["derived"]:
                derived_violations.append({"depends": tail, "on": head,
                                           "derived": [rows[tail]["derived"], rows[head]["derived"]]})

    cross_folder_hand = []
    folder_rank = {}
    for package, data in rename["report"]["packages"].items():
        for folder, info in data["folders"].items():
            folder_rank[folder] = info["rank"]
    for tail in graph.nodes:
        for head in closure[tail]:
            fa, fb = meta[tail]["folder"], meta[head]["folder"]
            if fa == fb:
                continue
            if folder_rank.get(fa, 0) < folder_rank.get(fb, 0):
                cross_folder_hand.append({"depends": tail, "on": head, "folders": [fa, fb],
                                          "ranks": [folder_rank.get(fa), folder_rank.get(fb)]})

    # The DEPENDER carries the error: its number claims a layer below something
    # it uses. The head is implicated, never convicted, since raising the head
    # would break its own consistent relations.
    guilty = {v["depends"] for v in hand_violations}
    guilty |= {v["depends"] for v in cross_folder_hand}

    classified = []
    counts = defaultdict(int)
    for path, row in rows.items():
        hand = row["hand"]
        origin = meta[path]["origin"]
        if origin == "generated":
            bucket = "generated"
        elif hand is None:
            bucket = "unnumbered"
        elif hand == row["derived"]:
            bucket = "agree"
        elif path in guilty:
            bucket = "hand_error"
        elif row["depth"] == 0 and row["out_degree"] == 0 and hand > 0:
            bucket = "metric_blind"
        else:
            bucket = "scale_shift"
        counts[bucket] += 1
        classified.append({
            "path": path, "package": meta[path]["package"], "folder": meta[path]["folder"],
            "hand": hand, "derived": row["derived"], "depth": row["depth"],
            "in_degree": row["in_degree"], "out_degree": row["out_degree"],
            "proposed": row["proposed"], "bucket": bucket,
            "external_imports": external_deps(path),
        })
    classified.sort(key=lambda r: (r["package"], r["folder"], r["derived"], -r["in_degree"], r["path"]))

    verdict = {
        "counts": dict(counts),
        "hand_violations_same_folder": hand_violations,
        "hand_violations_cross_folder": cross_folder_hand,
        "derived_violations": derived_violations,
        "rows": classified,
    }
    (OUT / "classification.json").write_text(json.dumps(verdict, indent=2, sort_keys=True) + "\n")

    lines = ["| package | current path | depth | hand | derived | in | out | proposed path | verdict |",
             "|---|---|---:|---:|---:|---:|---:|---|---|"]
    for row in classified:
        proposed = row["proposed"] if row["proposed"] != row["path"] else "(unchanged)"
        lines.append(f"| {row['package']} | `{row['path']}` | {row['depth']} | "
                     f"{'-' if row['hand'] is None else row['hand']} | {row['derived']} | "
                     f"{row['in_degree']} | {row['out_degree']} | `{proposed}` | {row['bucket']} |")
    (OUT / "rename_table.md").write_text("\n".join(lines) + "\n")

    print(json.dumps({k: v for k, v in verdict.items() if k != "rows"}, indent=2, sort_keys=True))
    print()
    print("\n".join(f"{k}\t{v}" for k, v in sorted(counts.items())))


if __name__ == "__main__":
    main()
